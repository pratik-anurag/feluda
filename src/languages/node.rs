use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "peerDependencies")]
    pub peer_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "optionalDependencies")]
    pub optional_dependencies: Option<HashMap<String, String>>,
}

impl PackageJson {
    /// Get all dependencies from package.json (production + dev + peer + optional)
    pub fn get_all_dependencies(&self) -> HashMap<String, String> {
        let mut all_dependencies: HashMap<String, String> = HashMap::new();

        if let Some(deps) = &self.dependencies {
            all_dependencies.extend(deps.clone());
        }
        if let Some(dev_deps) = &self.dev_dependencies {
            all_dependencies.extend(dev_deps.clone());
        }
        if let Some(peer_deps) = &self.peer_dependencies {
            all_dependencies.extend(peer_deps.clone());
        }
        if let Some(opt_deps) = &self.optional_dependencies {
            all_dependencies.extend(opt_deps.clone());
        }

        all_dependencies
    }

    /// Get production dependencies
    #[allow(dead_code)]
    pub fn get_production_dependencies(&self) -> HashMap<String, String> {
        self.dependencies.clone().unwrap_or_default()
    }
}

/// Recursive dependency resolver
struct DependencyResolver {
    resolved_cache: HashMap<String, PackageMetadata>,
    processing_stack: HashSet<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PackageMetadata {
    name: String,
    version: String,
    license: Option<String>,
    dependencies: HashMap<String, String>,
}

impl DependencyResolver {
    fn new() -> Self {
        Self {
            resolved_cache: HashMap::new(),
            processing_stack: HashSet::new(),
        }
    }

    fn resolve_recursive_dependencies(
        &mut self,
        package_json_path: &str,
        max_depth: usize,
    ) -> Result<HashMap<String, String>, String> {
        let root_package = self.parse_local_package_json(package_json_path)?;
        let mut all_dependencies = HashMap::new();

        let to_resolve = root_package.dependencies.clone();
        self.resolve_dependencies_recursive(to_resolve, &mut all_dependencies, 0, max_depth)?;

        Ok(all_dependencies)
    }

    fn resolve_dependencies_recursive(
        &mut self,
        dependencies: HashMap<String, String>,
        all_deps: &mut HashMap<String, String>,
        current_depth: usize,
        max_depth: usize,
    ) -> Result<(), String> {
        if current_depth >= max_depth {
            return Ok(());
        }

        for (name, version_spec) in dependencies {
            if all_deps.contains_key(&name) || self.processing_stack.contains(&name) {
                continue;
            }

            self.processing_stack.insert(name.clone());

            match self.resolve_package_metadata(&name, &version_spec) {
                Ok(metadata) => {
                    all_deps.insert(name.clone(), metadata.version.clone());
                    self.resolve_dependencies_recursive(
                        metadata.dependencies,
                        all_deps,
                        current_depth + 1,
                        max_depth,
                    )?;
                }
                Err(_) => {
                    all_deps.insert(name.clone(), clean_version_string(&version_spec));
                }
            }

            self.processing_stack.remove(&name);
        }

        Ok(())
    }

    fn resolve_package_metadata(
        &mut self,
        name: &str,
        version_spec: &str,
    ) -> Result<PackageMetadata, String> {
        let cache_key = format!("{}@{}", name, version_spec);

        if let Some(cached) = self.resolved_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let metadata = self.fetch_package_metadata_from_registry(name, version_spec)?;
        self.resolved_cache.insert(cache_key, metadata.clone());
        Ok(metadata)
    }

    fn fetch_package_metadata_from_registry(
        &self,
        name: &str,
        version_spec: &str,
    ) -> Result<PackageMetadata, String> {
        let clean_version = clean_version_string(version_spec);
        let url = if clean_version == "latest" || clean_version.is_empty() {
            format!("https://registry.npmjs.org/{}", name)
        } else {
            format!("https://registry.npmjs.org/{}/{}", name, clean_version)
        };

        let response =
            reqwest::blocking::get(&url).map_err(|e| format!("Registry request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Registry returned status: {}", response.status()));
        }

        let json: Value = response
            .json()
            .map_err(|e| format!("Failed to parse registry response: {}", e))?;

        self.parse_registry_metadata(&json, name, &clean_version)
    }

    fn parse_registry_metadata(
        &self,
        json: &Value,
        name: &str,
        requested_version: &str,
    ) -> Result<PackageMetadata, String> {
        let version_to_use = if requested_version == "latest" {
            json.get("dist-tags")
                .and_then(|tags| tags.get("latest"))
                .and_then(|v| v.as_str())
                .unwrap_or("latest")
        } else {
            requested_version
        };

        let version_data = if let Some(versions) = json.get("versions") {
            if let Some(specific_version) = versions.get(version_to_use) {
                specific_version
            } else {
                json
            }
        } else {
            json
        };

        let license = version_data
            .get("license")
            .and_then(|l| l.as_str())
            .or_else(|| {
                version_data
                    .get("licenses")
                    .and_then(|ls| ls.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|first| first.get("type"))
                    .and_then(|t| t.as_str())
            })
            .map(String::from);

        let dependencies = self.extract_dependencies_from_json(version_data, "dependencies");

        Ok(PackageMetadata {
            name: name.to_string(),
            version: version_to_use.to_string(),
            license,
            dependencies,
        })
    }

    fn extract_dependencies_from_json(
        &self,
        json: &Value,
        dep_type: &str,
    ) -> HashMap<String, String> {
        json.get(dep_type)
            .and_then(|deps| deps.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("*").to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_local_package_json(&self, path: &str) -> Result<PackageMetadata, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read package.json: {}", e))?;

        let json: Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse package.json: {}", e))?;

        let dependencies = self.extract_dependencies_from_json(&json, "dependencies");

        Ok(PackageMetadata {
            name: "root".to_string(),
            version: "0.0.0".to_string(),
            license: None,
            dependencies,
        })
    }
}

/// Analyze the licenses of JavaScript dependencies
pub fn analyze_js_licenses(package_json_path: &str) -> Vec<LicenseInfo> {
    analyze_js_licenses_with_options(package_json_path, true)
}

/// Analyze JavaScript dependencies
pub fn analyze_js_licenses_with_options(
    package_json_path: &str,
    include_transient: bool,
) -> Vec<LicenseInfo> {
    #[cfg(windows)]
    const NPM: &str = "npm.cmd";
    #[cfg(not(windows))]
    const NPM: &str = "npm";

    log(
        LogLevel::Info,
        &format!(
            "Analyzing JavaScript dependencies from: {} (transient: {})",
            package_json_path, include_transient
        ),
    );

    let project_root = Path::new(package_json_path)
        .parent()
        .unwrap_or(Path::new("."));

    let npm_ls_dependencies = get_full_dependency_tree(project_root, NPM);

    let all_dependencies = match npm_ls_dependencies {
        Ok(deps) if !deps.is_empty() => {
            log(
                LogLevel::Info,
                &format!("Found {} dependencies using npm ls", deps.len()),
            );
            deps
        }
        Ok(_) | Err(_) => {
            if include_transient {
                log(
                    LogLevel::Info,
                    "npm ls failed, using recursive dependency resolution",
                );

                let mut resolver = DependencyResolver::new();
                match resolver.resolve_recursive_dependencies(package_json_path, 8) {
                    Ok(deps) => {
                        log(
                            LogLevel::Info,
                            &format!(
                                "Found {} dependencies using recursive resolution",
                                deps.len()
                            ),
                        );
                        deps
                    }
                    Err(err) => {
                        log_error("Recursive resolution failed", &err);
                        parse_package_json_dependencies(package_json_path).unwrap_or_default()
                    }
                }
            } else {
                log(
                    LogLevel::Info,
                    "Using direct package.json parsing (no transient deps)",
                );
                parse_package_json_dependencies(package_json_path).unwrap_or_default()
            }
        }
    };

    if all_dependencies.is_empty() {
        log(LogLevel::Warn, "No dependencies found using any method");
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

    // Parallel process dependencies
    all_dependencies
        .par_iter()
        .map(|(name, version)| {
            log(
                LogLevel::Info,
                &format!("Checking license for JS dependency: {} ({})", name, version),
            );

            let license = get_license_from_package_json(project_root, name, version)
                .or_else(|| get_license_from_npm_view(NPM, name, version))
                .or_else(|| get_license_from_npm_registry_api(name, version))
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
                version: clean_version_string(version),
                license: Some(license),
                is_restrictive,
                compatibility: LicenseCompatibility::Unknown,
            }
        })
        .collect()
}

/// Parse dependencies directly from package.json when npm ls fails
fn parse_package_json_dependencies(
    package_json_path: &str,
) -> Result<HashMap<String, String>, String> {
    log(
        LogLevel::Info,
        &format!("Parsing dependencies directly from: {}", package_json_path),
    );

    let content = std::fs::read_to_string(package_json_path)
        .map_err(|e| format!("Failed to read package.json: {}", e))?;

    let package_json: PackageJson = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse package.json: {}", e))?;

    let all_deps = package_json.get_all_dependencies();

    log(
        LogLevel::Info,
        &format!(
            "Found {} total dependencies in package.json",
            all_deps.len()
        ),
    );

    log_debug("Dependencies from package.json", &all_deps);

    Ok(all_deps)
}

/// Clean version strings from package.json
fn clean_version_string(version: &str) -> String {
    version
        .trim_start_matches('^')
        .trim_start_matches('~')
        .trim_start_matches(">=")
        .trim_start_matches('>')
        .trim_start_matches("<=")
        .trim_start_matches('<')
        .trim_start_matches('=')
        .split_whitespace()
        .next()
        .unwrap_or(version)
        .to_string()
}

/// Get license info from npm registry API as additional fallback
fn get_license_from_npm_registry_api(package_name: &str, version: &str) -> Option<String> {
    log(
        LogLevel::Info,
        &format!("Trying npm registry API for {}", package_name),
    );

    let versions_to_try = if version == "latest" || version.is_empty() {
        vec!["latest"]
    } else {
        vec![version, "latest"]
    };

    for ver in versions_to_try {
        let url = if ver == "latest" {
            format!("https://registry.npmjs.org/{}", package_name)
        } else {
            format!("https://registry.npmjs.org/{}/{}", package_name, ver)
        };

        match reqwest::blocking::get(&url) {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<Value>() {
                        Ok(json) => {
                            let license_paths = [
                                vec!["license"],
                                vec!["licenses", "0", "type"],
                                vec!["licenses", "0"],
                                vec!["latest", "license"],
                            ];

                            for path in &license_paths {
                                if let Some(license_value) = get_nested_json_value(&json, path) {
                                    if let Some(license_str) = license_value.as_str() {
                                        if !license_str.is_empty() && license_str != "UNLICENSED" {
                                            log(
                                                LogLevel::Info,
                                                &format!(
                                                    "Found license via registry API for {}: {}",
                                                    package_name, license_str
                                                ),
                                            );
                                            return Some(license_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            log_error(
                                &format!("Failed to parse registry response for {}", package_name),
                                &err,
                            );
                        }
                    }
                }
            }
            Err(err) => {
                log_error(
                    &format!("Failed to fetch from registry for {}", package_name),
                    &err,
                );
            }
        }
    }

    None
}

/// Helper function to get nested JSON values
fn get_nested_json_value<'a>(json: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = json;
    for key in path {
        current = current.get(key)?;
    }
    Some(current)
}

/// Get the full dependency tree using npm ls command
fn get_full_dependency_tree(
    project_root: &Path,
    npm_cmd: &str,
) -> Result<HashMap<String, String>, String> {
    log(LogLevel::Info, "Getting full dependency tree using npm ls");

    let node_modules_path = project_root.join("node_modules");
    if !node_modules_path.exists() {
        log(
            LogLevel::Warn,
            &format!(
                "node_modules directory not found at: {}",
                node_modules_path.display()
            ),
        );
        return Err("node_modules directory does not exist".to_string());
    }

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

/// Get license information from a package's package.json
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
    let clean_version = clean_version_string(version);
    let package_spec = if clean_version == "latest" || clean_version.is_empty() {
        package_name.to_string()
    } else {
        format!("{}@{}", package_name, clean_version)
    };

    log(
        LogLevel::Info,
        &format!("Trying npm view for: {}", package_spec),
    );

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

/// Check if a license is considered restrictive
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
    fn test_package_json_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let package_json_path = temp_dir.path().join("package.json");

        let test_package_json = r#"{
            "name": "remix-example",
            "version": "0.1.0",
            "dependencies": {
                "@remix-run/express": "^2.16.0",
                "react": "^18.3.1"
            },
            "devDependencies": {
                "@remix-run/dev": "^2.16.0",
                "typescript": "^5.8.2"
            }
        }"#;

        std::fs::write(&package_json_path, test_package_json).unwrap();

        let result = parse_package_json_dependencies(package_json_path.to_str().unwrap()).unwrap();

        assert_eq!(result.len(), 4);
        assert!(result.contains_key("@remix-run/express"));
        assert!(result.contains_key("react"));
        assert!(result.contains_key("@remix-run/dev"));
        assert!(result.contains_key("typescript"));
    }

    #[test]
    fn test_clean_version_string() {
        assert_eq!(clean_version_string("^2.16.0"), "2.16.0");
        assert_eq!(clean_version_string("~18.3.1"), "18.3.1");
        assert_eq!(clean_version_string(">=1.0.0"), "1.0.0");
        assert_eq!(clean_version_string("<=2.0.0"), "2.0.0");
        assert_eq!(clean_version_string("1.2.3"), "1.2.3");
        assert_eq!(clean_version_string("latest"), "latest");
    }

    #[test]
    fn test_package_json_get_all_dependencies() {
        let package_json = PackageJson {
            dependencies: Some({
                let mut deps = HashMap::new();
                deps.insert("dep1".to_string(), "1.0.0".to_string());
                deps
            }),
            dev_dependencies: Some({
                let mut dev_deps = HashMap::new();
                dev_deps.insert("dev_dep1".to_string(), "1.0.0".to_string());
                dev_deps
            }),
            peer_dependencies: Some({
                let mut peer_deps = HashMap::new();
                peer_deps.insert("peer_dep1".to_string(), "1.0.0".to_string());
                peer_deps
            }),
            optional_dependencies: Some({
                let mut opt_deps = HashMap::new();
                opt_deps.insert("opt_dep1".to_string(), "1.0.0".to_string());
                opt_deps
            }),
        };

        let all_deps = package_json.get_all_dependencies();
        assert_eq!(all_deps.len(), 4);
        assert!(all_deps.contains_key("dep1"));
        assert!(all_deps.contains_key("dev_dep1"));
        assert!(all_deps.contains_key("peer_dep1"));
        assert!(all_deps.contains_key("opt_dep1"));
    }

    #[test]
    fn test_analyze_js_licenses_with_options() {
        let temp_dir = TempDir::new().unwrap();
        let package_json_path = temp_dir.path().join("package.json");

        std::fs::write(
            &package_json_path,
            r#"{"name": "test", "version": "1.0.0", "dependencies": {}}"#,
        )
        .unwrap();

        let result_with_transient =
            analyze_js_licenses_with_options(package_json_path.to_str().unwrap(), true);
        let result_without_transient =
            analyze_js_licenses_with_options(package_json_path.to_str().unwrap(), false);

        assert!(result_with_transient.is_empty());
        assert!(result_without_transient.is_empty());
    }
}
