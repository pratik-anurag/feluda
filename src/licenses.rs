use cargo_metadata::Package;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct LicenseInfo {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub is_restrictive: bool,
}

pub fn analyze_licenses(packages: Vec<Package>) -> Vec<LicenseInfo> {
    packages
        .into_iter()
        .map(|package| {
            let is_restrictive = match &package.license {
                Some(license) if license.contains("GPL") || license.contains("AGPL") => true,
                _ => false,
            };

            LicenseInfo {
                name: package.name,
                version: package.version.to_string(),
                license: package.license,
                is_restrictive,
            }
        })
        .collect()
}
