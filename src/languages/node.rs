use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

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

/// Structure representing a package.json file
#[derive(Deserialize, Serialize, Debug)]
pub struct PackageJson {
    pub dependencies: Option<HashMap<String, String>>,
    pub dev_dependencies: Option<HashMap<String, String>>,
}

impl PackageJson {
    #[allow(dead_code)]
    pub fn get_all_dependencies(self) -> HashMap<String, String> {
        let mut all_dependencies: HashMap<String, String> = HashMap::new();
        if let Some(deps) = self.dev_dependencies {
            all_dependencies.extend(deps)
        };
        if let Some(deps) = self.dependencies {
            all_dependencies.extend(deps)
        };
        all_dependencies
    }
}

/// Analyze the licenses of JavaScript dependencies
pub fn analyze_js_licenses(package_json_path: &str) -> Vec<LicenseInfo> {
    #[cfg(windows)]
    const NPM: &str = "npm.cmd";
    #[cfg(not(windows))]
    const NPM: &str = "npm";

    log(
        LogLevel::Info,
        &format!(
            "Analyzing JavaScript dependencies with full tree from: {}",
            package_json_path
        ),
    );

    let project_root = Path::new(package_json_path)
        .parent()
        .unwrap_or(Path::new("."));

    // First, try to get the full dependency tree using npm ls
    let all_dependencies = match get_full_dependency_tree(project_root, NPM) {
        Ok(deps) => {
            log(
                LogLevel::Info,
                &format!("Found {} dependencies using npm ls", deps.len()),
            );
            deps
        }
        Err(err) => {
            log(
                LogLevel::Warn,
                &format!("Failed to get full dependency tree via npm ls: {}. Falling back to node_modules scanning.", err),
            );
            // Fallback to scanning node_modules directory
            scan_node_modules(project_root)
        }
    };

    if all_dependencies.is_empty() {
        log(LogLevel::Warn, "No dependencies found");
        return Vec::new();
    }

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

    // Process dependencies in parallel for better performance
    all_dependencies
        .par_iter()
        .map(|(name, version)| {
            log(
                LogLevel::Info,
                &format!("Checking license for JS dependency: {} ({})", name, version),
            );

            // Get license info from package.json
            let license = get_license_from_package_json(project_root, name, version)
                .or_else(|| get_license_from_npm_view(NPM, name, version))
                .unwrap_or_else(|| "Unknown (failed to retrieve)".to_string());

            log(
                LogLevel::Info,
                &format!("License for {} ({}): {}", name, version, license),
            );

            let is_restrictive = is_license_restrictive(&Some(license.clone()), &known_licenses);

            if is_restrictive {
                log(
                    LogLevel::Warn,
                    &format!("Restrictive license found: {} for {}", license, name),
                );
            }

            LicenseInfo {
                name: name.clone(),
                version: version.clone(),
                license: Some(license),
                is_restrictive,
                compatibility: LicenseCompatibility::Unknown,
            }
        })
        .collect()
}

/// Get the full dependency tree using npm ls command
fn get_full_dependency_tree(
    project_root: &Path,
    npm_cmd: &str,
) -> Result<HashMap<String, String>, String> {
    log(LogLevel::Info, "Getting full dependency tree using npm ls");

    let output = Command::new(npm_cmd)
        .arg("ls")
        .arg("--json")
        .arg("--depth=0")
        .arg("--production")
        .arg("--dev")
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("Failed to execute npm ls: {}", e))?;

    let stdout_str = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        log(
            LogLevel::Warn,
            &format!(
                "npm ls returned non-zero exit code. stdout: {}, stderr: {}",
                stdout_str, stderr_str
            ),
        );
    }

    let json: Value = serde_json::from_str(&stdout_str)
        .map_err(|e| format!("Failed to parse npm ls output as JSON: {}", e))?;

    let mut all_deps = HashMap::new();

    // Parse the dependency tree recursively
    if let Some(dependencies) = json.get("dependencies").and_then(|d| d.as_object()) {
        parse_dependency_tree_recursive(dependencies, &mut all_deps);
    }

    log(
        LogLevel::Info,
        &format!("Parsed {} unique dependencies from npm ls", all_deps.len()),
    );

    Ok(all_deps)
}

/// Recursively parse the dependency tree from npm ls
fn parse_dependency_tree_recursive(
    dependencies: &serde_json::Map<String, Value>,
    all_deps: &mut HashMap<String, String>,
) {
    for (name, dep_info) in dependencies {
        if let Some(version) = dep_info.get("version").and_then(|v| v.as_str()) {
            if !all_deps.contains_key(name) {
                all_deps.insert(name.clone(), version.to_string());
                log(
                    LogLevel::Info,
                    &format!("Added dependency: {} v{}", name, version),
                );
            }

            // Recursively process nested dependencies
            if let Some(nested_deps) = dep_info.get("dependencies").and_then(|d| d.as_object()) {
                parse_dependency_tree_recursive(nested_deps, all_deps);
            }
        }
    }
}

/// Fallback method: scan node_modules directory for all packages
fn scan_node_modules(project_root: &Path) -> HashMap<String, String> {
    log(
        LogLevel::Info,
        "Scanning node_modules directory for dependencies",
    );

    let node_modules_path = project_root.join("node_modules");
    let mut dependencies = HashMap::new();

    if !node_modules_path.exists() {
        log(
            LogLevel::Warn,
            &format!(
                "node_modules directory not found at: {}",
                node_modules_path.display()
            ),
        );
        return dependencies;
    }

    // Scan top-level packages
    scan_node_modules_recursive(&node_modules_path, &mut dependencies, 0);

    log(
        LogLevel::Info,
        &format!(
            "Found {} dependencies by scanning node_modules",
            dependencies.len()
        ),
    );

    dependencies
}

/// Recursively scan node_modules directory
fn scan_node_modules_recursive(
    node_modules_path: &Path,
    dependencies: &mut HashMap<String, String>,
    depth: usize,
) {
    // Limit depth for performance
    if depth > 10 {
        return;
    }

    let entries = match std::fs::read_dir(node_modules_path) {
        Ok(entries) => entries,
        Err(err) => {
            log(
                LogLevel::Warn,
                &format!(
                    "Failed to read directory {}: {}",
                    node_modules_path.display(),
                    err
                ),
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        let file_name = entry.file_name();
        let name = match file_name.to_str() {
            Some(name) => name,
            None => continue,
        };

        if name.starts_with('.') || name == "node_modules" {
            continue;
        }

        if path.is_dir() {
            if name.starts_with('@') {
                scan_scoped_packages(&path, dependencies, depth);
            } else if let Some(version) = get_package_version_from_package_json(&path) {
                dependencies.insert(name.to_string(), version);

                let nested_node_modules = path.join("node_modules");
                if nested_node_modules.exists() {
                    scan_node_modules_recursive(&nested_node_modules, dependencies, depth + 1);
                }
            }
        }
    }
}

/// Scan scoped packages
fn scan_scoped_packages(
    scope_path: &Path,
    dependencies: &mut HashMap<String, String>,
    depth: usize,
) {
    let scope_name = scope_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let entries = match std::fs::read_dir(scope_path) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        let file_name = entry.file_name();
        let package_name = match file_name.to_str() {
            Some(name) => name,
            None => continue,
        };

        if path.is_dir() {
            let full_name = format!("{}/{}", scope_name, package_name);
            if let Some(version) = get_package_version_from_package_json(&path) {
                dependencies.insert(full_name, version);

                let nested_node_modules = path.join("node_modules");
                if nested_node_modules.exists() {
                    scan_node_modules_recursive(&nested_node_modules, dependencies, depth + 1);
                }
            }
        }
    }
}

/// Get package version from package.json file
fn get_package_version_from_package_json(package_path: &Path) -> Option<String> {
    let package_json_path = package_path.join("package.json");

    let content = std::fs::read_to_string(package_json_path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;

    json.get("version")?.as_str().map(String::from)
}

/// Get license information from a package's package.json file
fn get_license_from_package_json(
    project_root: &Path,
    package_name: &str,
    _version: &str,
) -> Option<String> {
    let package_path = if package_name.starts_with('@') {
        let parts: Vec<&str> = package_name.splitn(2, '/').collect();
        if parts.len() == 2 {
            project_root
                .join("node_modules")
                .join(parts[0])
                .join(parts[1])
                .join("package.json")
        } else {
            return None;
        }
    } else {
        project_root
            .join("node_modules")
            .join(package_name)
            .join("package.json")
    };

    let content = std::fs::read_to_string(package_path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;

    if let Some(license) = json.get("license").and_then(|l| l.as_str()) {
        if !license.is_empty() && license != "UNLICENSED" {
            log(
                LogLevel::Info,
                &format!(
                    "Found license in package.json for {}: {}",
                    package_name, license
                ),
            );
            return Some(license.to_string());
        }
    }

    // Check for licenses array.
    if let Some(licenses) = json.get("licenses").and_then(|l| l.as_array()) {
        if let Some(first_license) = licenses.first() {
            if let Some(license_type) = first_license.get("type").and_then(|t| t.as_str()) {
                log(
                    LogLevel::Info,
                    &format!(
                        "Found license in licenses array for {}: {}",
                        package_name, license_type
                    ),
                );
                return Some(license_type.to_string());
            }
        }
    }

    None
}

/// Fallback: get license using npm view command
fn get_license_from_npm_view(npm_cmd: &str, package_name: &str, version: &str) -> Option<String> {
    let package_spec = if version == "latest" || version.is_empty() {
        package_name.to_string()
    } else {
        format!("{}@{}", package_name, version)
    };

    let output = Command::new(npm_cmd)
        .arg("view")
        .arg(&package_spec)
        .arg("license")
        .arg("--json")
        .output()
        .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log(
            LogLevel::Warn,
            &format!("npm view failed for {}: {}", package_spec, stderr),
        );
        return None;
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    if let Ok(json) = serde_json::from_str::<Value>(&output_str) {
        if let Some(license) = json.as_str() {
            return Some(license.to_string());
        }
    }

    let license = output_str.trim().trim_matches('"');
    if !license.is_empty() && license != "undefined" {
        Some(license.to_string())
    } else {
        None
    }
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

    #[test]
    fn test_package_json_get_all_dependencies() {
        let package_json = PackageJson {
            dependencies: Some({
                let mut deps = std::collections::HashMap::new();
                deps.insert("dep1".to_string(), "1.0.0".to_string());
                deps.insert("dep2".to_string(), "2.0.0".to_string());
                deps
            }),
            dev_dependencies: Some({
                let mut dev_deps = std::collections::HashMap::new();
                dev_deps.insert("dev_dep1".to_string(), "1.0.0".to_string());
                dev_deps
            }),
        };

        let all_deps = package_json.get_all_dependencies();
        assert_eq!(all_deps.len(), 3);
        assert!(all_deps.contains_key("dep1"));
        assert!(all_deps.contains_key("dep2"));
        assert!(all_deps.contains_key("dev_dep1"));
    }

    #[test]
    fn test_analyze_js_licenses_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let package_json_path = temp_dir.path().join("package.json");

        std::fs::write(
            &package_json_path,
            r#"{
                "name": "test-project",
                "version": "1.0.0"
            }"#,
        )
        .unwrap();

        let result = analyze_js_licenses(package_json_path.to_str().unwrap());
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_package_version_from_package_json() {
        let temp_dir = TempDir::new().unwrap();
        let package_path = temp_dir.path();

        std::fs::write(
            package_path.join("package.json"),
            r#"{"name": "test", "version": "1.2.3"}"#,
        )
        .unwrap();

        let version = get_package_version_from_package_json(package_path);
        assert_eq!(version, Some("1.2.3".to_string()));
    }

    #[test]
    fn test_get_license_from_package_json() {
        let temp_dir = TempDir::new().unwrap();
        let node_modules = temp_dir.path().join("node_modules");
        let package_dir = node_modules.join("test-package");

        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            package_dir.join("package.json"),
            r#"{"name": "test-package", "version": "1.0.0", "license": "MIT"}"#,
        )
        .unwrap();

        let license = get_license_from_package_json(temp_dir.path(), "test-package", "1.0.0");
        assert_eq!(license, Some("MIT".to_string()));
    }
}
