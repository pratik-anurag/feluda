use cargo_metadata::Package;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct LicenseInfo {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub is_restrictive: bool,
}

impl LicenseInfo {
    pub fn get_license(&self) -> String {
        match &self.license {
            Some(license_name) => {
                String::from(license_name)
            }
            None => {
                String::from("No License")
            }
        }
    }
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
