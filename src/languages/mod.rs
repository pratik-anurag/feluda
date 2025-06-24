//! Language-specific parsing and license analysis modules

pub mod go;
pub mod node;
pub mod python;
pub mod rust;

// Re-export commonly used types and functions for backward compatibility
// TODO: Remove when 1.8.5 is no longer supported
pub use go::{
    analyze_go_licenses, fetch_license_for_go_dependency, get_go_dependencies, GoPackages,
};
pub use node::{analyze_js_licenses, PackageJson};
pub use python::{analyze_python_licenses, fetch_license_for_python_dependency};
pub use rust::analyze_rust_licenses;

use crate::licenses::LicenseInfo;
use std::path::Path;

/// Common trait for language-specific dependency parsers
#[allow(dead_code)]
pub trait LanguageParser {
    /// Parse dependencies from a project file and return license information
    fn parse_dependencies(
        &self,
        project_path: &Path,
    ) -> crate::debug::FeludaResult<Vec<LicenseInfo>>;

    /// Get the name of the language
    fn language_name(&self) -> &'static str;

    /// Get the typical project files
    fn supported_files(&self) -> &'static [&'static str];
}

/// Language identification
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Language {
    Rust(&'static str),
    Node(&'static str),
    Go(&'static str),
    Python(&'static [&'static str]),
}

impl Language {
    pub fn from_file_name(file_name: &str) -> Option<Self> {
        match file_name {
            "Cargo.toml" => Some(Language::Rust("Cargo.toml")),
            "package.json" => Some(Language::Node("package.json")),
            "go.mod" => Some(Language::Go("go.mod")),
            _ => {
                if PYTHON_PATHS.contains(&file_name) {
                    Some(Language::Python(&PYTHON_PATHS[..]))
                } else {
                    None
                }
            }
        }
    }
}

/// Python project file patterns
pub const PYTHON_PATHS: [&str; 4] = [
    "requirements.txt",
    "Pipfile.lock",
    "pip_freeze.txt",
    "pyproject.toml",
];
