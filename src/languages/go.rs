use rayon::prelude::*;
use regex::Regex;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::fs;
use std::thread::sleep;
use std::time::Duration;

use crate::cli;
use crate::config;
use crate::debug::{log, log_debug, log_error, FeludaResult, LogLevel};
use crate::licenses::{LicenseCompatibility, LicenseInfo};

/// License Info
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct License {
    title: String,            // The full name of the license
    spdx_id: String,          // The SPDX identifier for the license
    permissions: Vec<String>, // A list of permissions granted by the license
    conditions: Vec<String>,  // A list of conditions that must be met under the license
    limitations: Vec<String>, // A list of limitations imposed by the license
}

/// Go package information
#[derive(Debug)]
pub struct GoPackages {
    pub name: String,
    pub version: String,
}

/// Analyze the licenses of Go dependencies
pub fn analyze_go_licenses(go_mod_path: &str) -> Vec<LicenseInfo> {
    log(
        LogLevel::Info,
        &format!("Analyzing Go dependencies from: {}", go_mod_path),
    );

    let known_licenses = match fetch_licenses_from_github() {
        Ok(licenses) => {
            log(
                LogLevel::Info,
                &format!("Fetched {} known licenses from GitHub", licenses.len()),
            );
            licenses
        }
        Err(err) => {
            log_error("Failed to fetch licenses from GitHub", &err);
            HashMap::new()
        }
    };

    let content = match fs::read_to_string(go_mod_path) {
        Ok(content) => content,
        Err(err) => {
            log_error(
                &format!("Failed to read go.mod file: {}", go_mod_path),
                &err,
            );
            return Vec::new();
        }
    };

    let dependencies = get_go_dependencies(content);
    log(
        LogLevel::Info,
        &format!("Found {} Go dependencies", dependencies.len()),
    );
    log_debug("Go dependencies", &dependencies);

    dependencies
        .par_iter()
        .map(|dependency| -> LicenseInfo {
            let name = dependency.name.clone();
            let version = dependency.version.clone();

            log(
                LogLevel::Info,
                &format!("Fetching license for Go dependency: {} ({})", name, version),
            );

            let license_result = fetch_license_for_go_dependency(name.as_str(), version.as_str());
            let license = Some(license_result);

            let is_restrictive = is_license_restrictive(&license, &known_licenses);

            if is_restrictive {
                log(
                    LogLevel::Warn,
                    &format!("Restrictive license found: {:?} for {}", license, name),
                );
            }

            LicenseInfo {
                name,
                version,
                license,
                is_restrictive,
                compatibility: LicenseCompatibility::Unknown,
            }
        })
        .collect()
}

/// Parse Go dependencies from go.mod content
pub fn get_go_dependencies(content_string: String) -> Vec<GoPackages> {
    log(LogLevel::Info, "Parsing Go dependencies");

    let re_comment = match Regex::new(r"(?m)^(.*?)\s*(//|#).*?$") {
        Ok(re) => re,
        Err(err) => {
            log_error("Failed to compile comment regex", &err);
            return Vec::new();
        }
    };

    let cleaned = re_comment.replace_all(content_string.as_str(), "$1");

    let re = match Regex::new(
        r"require\s*(?:\(\s*)?((?:[\w./-]+\s+v[\d][\w\d.-]+(?:-\w+)?(?:\+\w+)?\s*)+)\)?",
    ) {
        Ok(re) => re,
        Err(err) => {
            log_error("Failed to compile require regex", &err);
            return Vec::new();
        }
    };

    let re_dependency = match Regex::new(r"([\w./-]+)\s+(v[\d]+(?:\.\d+)*(?:-\S+)?)") {
        Ok(re) => re,
        Err(err) => {
            log_error("Failed to compile dependency regex", &err);
            return Vec::new();
        }
    };

    let mut dependency = vec![];
    for cap in re.captures_iter(&cleaned) {
        let dependency_block = &cap[1];
        log_debug("Dependency block", &dependency_block);

        for dep_cap in re_dependency.captures_iter(dependency_block) {
            let name = dep_cap[1].to_string();
            let version = dep_cap[2].to_string();

            log(
                LogLevel::Info,
                &format!("Found Go dependency: {} ({})", name, version),
            );

            dependency.push(GoPackages { name, version });
        }
    }

    log(
        LogLevel::Info,
        &format!("Parsed {} Go dependencies", dependency.len()),
    );
    dependency
}

/// Fetch the license for a Go dependency from the Go Package Index (pkg.go.dev)
pub fn fetch_license_for_go_dependency(
    name: impl Into<String>,
    _version: impl Into<String>,
) -> String {
    let name = name.into();
    let _version = _version.into();

    let api_url = format!("https://pkg.go.dev/{}?tab=licenses", name);
    log(
        LogLevel::Info,
        &format!("Fetching license from Go Package Index: {}", api_url),
    );

    let client = match Client::builder()
        .user_agent("feluda.anirudha.dev/1")
        .connect_timeout(Duration::from_secs(60))
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            log_error("Failed to build HTTP client", &err);
            return "Unknown".into();
        }
    };

    let mut attempts = 0;
    let max_attempts = 7; // Retry max 7 times. Thala for a reason 🙌
    let wait_time = 12;

    while attempts < max_attempts {
        let response = client
            .get(&api_url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (compatible; Feluda-Bot/1.0; +https://github.com/anistark/feluda)",
            )
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Referer", "https://pkg.go.dev/")
            .send();

        match response {
            Ok(response) => {
                let status = response.status();
                log(
                    LogLevel::Info,
                    &format!("Go Package Index API response status: {}", status),
                );

                if status.as_u16() == 429 {
                    log(
                        LogLevel::Warn,
                        &format!(
                            "Received 429 Too Many Requests, retrying... (attempt {}/{})",
                            attempts + 1,
                            max_attempts
                        ),
                    );
                    sleep(Duration::from_secs(wait_time));
                    attempts += 1;
                    continue;
                }

                if status.is_success() {
                    match response.text() {
                        Ok(html_content) => {
                            if let Some(license) = extract_license_from_html(&html_content) {
                                log(
                                    LogLevel::Info,
                                    &format!("License found for {}: {}", name, license),
                                );
                                return license;
                            } else {
                                log(
                                    LogLevel::Warn,
                                    &format!("No license found in HTML for {}", name),
                                );
                            }
                        }
                        Err(err) => {
                            log_error(
                                &format!("Failed to extract HTML content for {}", name),
                                &err,
                            );
                        }
                    }
                } else {
                    log(
                        LogLevel::Error,
                        &format!("Unexpected HTTP status: {} for {}", status, name),
                    );
                }

                break;
            }
            Err(err) => {
                log_error(&format!("Failed to fetch metadata for {}", name), &err);
                break;
            }
        }
    }

    log(
        LogLevel::Warn,
        &format!(
            "Unable to determine license for {} after {} attempts",
            name, attempts
        ),
    );
    "Unknown".into()
}

/// Extract license information from the HTML content
fn extract_license_from_html(html: &str) -> Option<String> {
    log(LogLevel::Info, "Extracting license from HTML content");

    let document = Html::parse_document(html);

    // Select the <section> with class "License"
    let section_selector = match Selector::parse("section.License") {
        Ok(selector) => selector,
        Err(err) => {
            log_error("Failed to parse section selector", &err);
            return None;
        }
    };

    let div_selector = match Selector::parse("h2.go-textTitle div") {
        Ok(selector) => selector,
        Err(err) => {
            log_error("Failed to parse div selector", &err);
            return None;
        }
    };

    if let Some(section) = document.select(&section_selector).next() {
        if let Some(div) = section.select(&div_selector).next() {
            let license_text = div.text().collect::<Vec<_>>().join(" ").trim().to_string();
            log(
                LogLevel::Info,
                &format!("License found in HTML: {}", license_text),
            );
            return Some(license_text);
        } else {
            log(LogLevel::Warn, "Found section but no license div");
        }
    } else {
        log(LogLevel::Warn, "No license section found in HTML");
    }

    None
}

/// Check if a license is considered restrictive based on configuration and known licenses
fn is_license_restrictive(
    license: &Option<String>,
    known_licenses: &HashMap<String, License>,
) -> bool {
    log(
        LogLevel::Info,
        &format!("Checking if license is restrictive: {:?}", license),
    );

    let config = match config::load_config() {
        Ok(cfg) => {
            log(LogLevel::Info, "Successfully loaded configuration");
            cfg
        }
        Err(e) => {
            log_error("Error loading configuration", &e);
            log(LogLevel::Warn, "Using default configuration");
            config::FeludaConfig::default()
        }
    };

    if license.as_deref() == Some("No License") {
        log(
            LogLevel::Warn,
            "No license specified, considering as restrictive",
        );
        return true;
    }

    if let Some(license_str) = license {
        log_debug(
            "Checking against known licenses",
            &known_licenses.keys().collect::<Vec<_>>(),
        );

        if let Some(license_data) = known_licenses.get(license_str) {
            log_debug("Found license data", license_data);

            const CONDITIONS: [&str; 2] = ["source-disclosure", "network-use-disclosure"];
            let is_restrictive = CONDITIONS
                .iter()
                .any(|&condition| license_data.conditions.contains(&condition.to_string()));

            if is_restrictive {
                log(
                    LogLevel::Warn,
                    &format!("License {} is restrictive due to conditions", license_str),
                );
            } else {
                log(
                    LogLevel::Info,
                    &format!("License {} is not restrictive", license_str),
                );
            }

            return is_restrictive;
        } else {
            // Check against user-configured restrictive licenses
            log_debug(
                "Checking against configured restrictive licenses",
                &config.licenses.restrictive,
            );

            let is_restrictive = config
                .licenses
                .restrictive
                .iter()
                .any(|restrictive_license| license_str.contains(restrictive_license));

            if is_restrictive {
                log(
                    LogLevel::Warn,
                    &format!(
                        "License {} matches restrictive pattern in config",
                        license_str
                    ),
                );
            } else {
                log(
                    LogLevel::Info,
                    &format!(
                        "License {} does not match any restrictive pattern",
                        license_str
                    ),
                );
            }

            return is_restrictive;
        }
    }

    log(LogLevel::Warn, "No license information available");
    false
}

/// Fetch license data from GitHub's choosealicense repository
fn fetch_licenses_from_github() -> FeludaResult<HashMap<String, License>> {
    log(
        LogLevel::Info,
        "Fetching licenses from GitHub choosealicense repository",
    );

    let licenses_url =
        "https://raw.githubusercontent.com/github/choosealicense.com/gh-pages/_licenses/";

    let licenses_map = cli::with_spinner("Fetching licenses from GitHub", |indicator| {
        let mut licenses_map = HashMap::new();
        let mut license_count = 0;

        match reqwest::blocking::get(licenses_url) {
            Ok(response) => {
                if !response.status().is_success() {
                    let status = response.status();
                    log(
                        LogLevel::Error,
                        &format!("GitHub API returned error status: {}", status),
                    );
                    return licenses_map;
                }

                match response.text() {
                    Ok(content) => {
                        indicator.update_progress("parsing license list");

                        let mut license_files = Vec::new();
                        for line in content.lines() {
                            if line.ends_with(".txt") {
                                license_files.push(line.to_string());
                            }
                        }

                        let total_licenses = license_files.len();
                        indicator.update_progress(&format!("found {} licenses", total_licenses));

                        for (idx, line) in license_files.iter().enumerate() {
                            let license_name = line.replace(".txt", "");
                            let license_url = format!("{}{}", licenses_url, line);

                            log(
                                LogLevel::Info,
                                &format!("Fetching license: {}", license_name),
                            );

                            indicator.update_progress(&format!(
                                "processing {}/{}: {}",
                                idx + 1,
                                total_licenses,
                                license_name
                            ));

                            let license_response = match reqwest::blocking::get(&license_url) {
                                Ok(res) => res,
                                Err(err) => {
                                    log_error(
                                        &format!(
                                            "Failed to fetch license content for {}",
                                            license_name
                                        ),
                                        &err,
                                    );
                                    continue;
                                }
                            };

                            if !license_response.status().is_success() {
                                log(
                                    LogLevel::Error,
                                    &format!(
                                        "Failed to fetch license {}: HTTP {}",
                                        license_name,
                                        license_response.status()
                                    ),
                                );
                                continue;
                            }

                            let license_content = match license_response.text() {
                                Ok(text) => text,
                                Err(err) => {
                                    log_error(
                                        &format!(
                                            "Failed to read license content for {}",
                                            license_name
                                        ),
                                        &err,
                                    );
                                    continue;
                                }
                            };

                            match serde_yaml::from_str::<License>(&license_content) {
                                Ok(license) => {
                                    licenses_map.insert(license_name, license);
                                    license_count += 1;
                                }
                                Err(err) => {
                                    log_error(
                                        &format!(
                                            "Failed to parse license content for {}",
                                            license_name
                                        ),
                                        &err,
                                    );
                                }
                            }
                        }

                        indicator.update_progress(&format!("processed {} licenses", license_count));
                    }
                    Err(err) => {
                        log_error("Failed to read response text", &err);
                    }
                }
            }
            Err(err) => {
                log_error("Failed to fetch licenses list", &err);
            }
        }

        log(
            LogLevel::Info,
            &format!("Successfully fetched {} licenses", license_count),
        );
        licenses_map
    });

    Ok(licenses_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_go_dependencies() {
        let content = r#"require (
            github.com/user/repo v1.0.0
            github.com/another/pkg v2.3.4
        )"#;

        let deps = get_go_dependencies(content.to_string());
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].name, "github.com/user/repo");
        assert_eq!(deps[0].version, "v1.0.0");
    }

    #[test]
    fn test_get_go_dependencies_single_require() {
        let content = "require github.com/test/pkg v1.0.0".to_string();
        let deps = get_go_dependencies(content);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "github.com/test/pkg");
        assert_eq!(deps[0].version, "v1.0.0");
    }

    #[test]
    fn test_get_go_dependencies_with_comments() {
        let content = r#"require (
    github.com/user/repo v1.0.0 // This is a comment
    github.com/another/pkg v2.0.0 # This is also a comment
)"#
        .to_string();

        let deps = get_go_dependencies(content);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].name, "github.com/user/repo");
        assert_eq!(deps[1].name, "github.com/another/pkg");
    }

    #[test]
    fn test_get_go_dependencies_complex_versions() {
        let content = r#"require (
    github.com/user/repo v1.2.3-beta+build
    github.com/another/pkg v2.0.0-rc.1
    github.com/third/mod v0.1.0-alpha
)"#
        .to_string();

        let deps = get_go_dependencies(content);
        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].version, "v1.2.3-beta+build");
        assert_eq!(deps[1].version, "v2.0.0-rc.1");
        assert_eq!(deps[2].version, "v0.1.0-alpha");
    }

    #[test]
    fn test_get_go_dependencies_empty_content() {
        let content = "".to_string();
        let deps = get_go_dependencies(content);
        assert!(deps.is_empty());
    }

    #[test]
    fn test_extract_license_from_html() {
        let html_content = r#"
            <html>
                <body>
                    <section class="License">
                        <h2 class="go-textTitle">
                            <div>MIT</div>
                        </h2>
                    </section>
                </body>
            </html>
        "#;

        let license = extract_license_from_html(html_content);
        assert_eq!(license, Some("MIT".to_string()));
    }

    #[test]
    fn test_extract_license_from_html_no_license() {
        let html_content = r#"
            <html>
                <body>
                    <span class="go-Main-headerDetailItem" data-test-id="UnitHeader-licenses">
                    </span>
                </body>
            </html>
        "#;
        let license = extract_license_from_html(html_content);
        assert_eq!(license, None);
    }

    #[test]
    fn test_fetch_license_for_go_dependency_error_handling() {
        // Test with invalid package name
        let result = fetch_license_for_go_dependency("invalid/package/name", "v1.0.0");
        assert_eq!(result, "Unknown");
    }

    #[test]
    fn test_go_packages_debug() {
        let go_package = GoPackages {
            name: "github.com/test/package".to_string(),
            version: "v1.0.0".to_string(),
        };

        let debug_str = format!("{:?}", go_package);
        assert!(debug_str.contains("github.com/test/package"));
        assert!(debug_str.contains("v1.0.0"));
    }
}
