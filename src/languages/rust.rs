use cargo_metadata::Package;
use rayon::prelude::*;
use std::collections::HashMap;

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

/// Analyze the licenses of Rust dependencies from Cargo packages
pub fn analyze_rust_licenses(packages: Vec<Package>) -> Vec<LicenseInfo> {
    if packages.is_empty() {
        log(
            LogLevel::Warn,
            "No Rust packages found for license analysis",
        );
        return vec![];
    }

    log(
        LogLevel::Info,
        &format!("Analyzing licenses for {} Rust packages", packages.len()),
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

    packages
        .par_iter()
        .map(|package| {
            log(
                LogLevel::Info,
                &format!("Analyzing package: {} ({})", package.name, package.version),
            );

            let is_restrictive = is_license_restrictive(&package.license, &known_licenses);

            if is_restrictive {
                log(
                    LogLevel::Warn,
                    &format!(
                        "Restrictive license found: {:?} for {}",
                        package.license, package.name
                    ),
                );
            }

            LicenseInfo {
                name: package.name.to_string(),
                version: package.version.to_string(),
                license: package.license.clone(),
                is_restrictive,
                compatibility: LicenseCompatibility::Unknown,
            }
        })
        .collect()
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
    use tempfile::TempDir;

    fn setup() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_analyze_rust_licenses_empty() {
        let packages = vec![];
        let result = analyze_rust_licenses(packages);
        assert!(result.is_empty());
    }

    #[test]
    fn test_license_restrictive_with_default_config() {
        temp_env::with_var("FELUDA_LICENSES_RESTRICTIVE", None::<&str>, || {
            let dir = setup();
            std::env::set_current_dir(dir.path()).unwrap();

            let known_licenses = HashMap::new();
            assert!(is_license_restrictive(
                &Some("GPL-3.0".to_string()),
                &known_licenses
            ));
            assert!(!is_license_restrictive(
                &Some("MIT".to_string()),
                &known_licenses
            ));
        });
    }

    #[test]
    fn test_license_restrictive_no_license() {
        temp_env::with_var("FELUDA_LICENSES_RESTRICTIVE", None::<&str>, || {
            let dir = setup();
            std::env::set_current_dir(dir.path()).unwrap();

            let known_licenses = HashMap::new();
            assert!(is_license_restrictive(
                &Some("No License".to_string()),
                &known_licenses
            ));
        });
    }
}
