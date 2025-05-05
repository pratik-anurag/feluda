use crate::cli::CiFormat;
use crate::licenses::LicenseInfo;
use std::collections::HashMap;
use std::fs;

pub fn generate_report(
    data: Vec<LicenseInfo>,
    json: bool,
    verbose: bool,
    strict: bool,
    ci_format: Option<CiFormat>,
    output_file: Option<String>,
) -> bool {
    let total_packages = data.len();
    let filtered_data: Vec<LicenseInfo> = if strict {
        data.into_iter()
            .filter(|info| *info.is_restrictive())
            .collect()
    } else {
        data
    };

    let has_restrictive = filtered_data.iter().any(|info| *info.is_restrictive());

    if filtered_data.is_empty() {
        println!(
            "\n🎉 All dependencies passed the license check! No restrictive licenses found.\n"
        );
        return false;
    }

    // Handle CI output format if specified
    if let Some(format) = ci_format {
        match format {
            CiFormat::Github => output_github_format(&filtered_data, output_file.as_deref()),
            CiFormat::Jenkins => output_jenkins_format(&filtered_data, output_file.as_deref()),
        }
    } else if json {
        // Existing JSON output
        let json_output =
            serde_json::to_string_pretty(&filtered_data).expect("Failed to serialize data");
        println!("{}", json_output);
    } else {
        // Existing plain text output
        let mut restrictive_licenses: Vec<LicenseInfo> = Vec::new();
        let mut license_count: HashMap<Option<String>, usize> = HashMap::new();
        for info in filtered_data {
            if verbose {
                println!(
                    "Name: {}, Version: {}, License: {:?}, Restrictive: {}",
                    info.name,
                    info.version,
                    info.get_license(),
                    info.is_restrictive
                );
            }

            if *info.is_restrictive() {
                // Add to a separate array or handle as needed
                restrictive_licenses.push(info);
            } else {
                *license_count.entry(info.license.clone()).or_insert(0) += 1;
            }
        }

        println!("{:<49} {:<5}", "License Type", "Dependencies");
        println!("{:<53} {:<5}", "---------------------", "------------");
        for (license, count) in license_count {
            println!(
                "{:<58} {:<5}",
                license.unwrap_or_else(|| "Unknown".to_string()),
                count
            );
        }
        println!("\nTotal dependencies scanned: {}", total_packages);
        if restrictive_licenses.is_empty() {
            println!("\n✅ No restrictive licenses found! 🎉\n");
        } else {
            println!("\n⚠️ Warning: Restrictive licenses may have been found! ⚠️");
            println!("\n{:<50} {:<5}", "Restrictive License Type", "Dependencies");
            println!("{:<50} {:<5}", "---------------------", "------------");
            for info in restrictive_licenses {
                println!(
                    "{:<50} {:<5}",
                    info.license.unwrap_or_else(|| "Unknown".to_string()),
                    info.name
                );
            }
        }
    }

    has_restrictive
}

fn output_github_format(license_info: &[LicenseInfo], output_path: Option<&str>) {
    // Create GitHub Actions compatible output
    let mut output = String::new();

    // GitHub Actions workflow commands format
    // https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions

    for info in license_info {
        if *info.is_restrictive() {
            // Format: ::warning title={title}::{message}
            output.push_str(&format!(
                "::warning title=Restrictive License::Dependency '{}@{}' has restrictive license: {}\n",
                info.name(),
                info.version(),
                info.get_license()
            ));
        }
    }

    // Append summary using notice
    let restrictive_count = license_info.iter().filter(|i| *i.is_restrictive()).count();
    output.push_str(&format!(
        "::notice title=License Check Summary::Found {} dependencies with restrictive licenses out of {} total\n",
        restrictive_count,
        license_info.len()
    ));

    // Output to file or stdout
    if let Some(path) = output_path {
        fs::write(path, output).expect("Failed to write GitHub Actions output file");
        println!("GitHub Actions output written to: {}", path);
    } else {
        print!("{}", output);
    }
}

fn output_jenkins_format(license_info: &[LicenseInfo], output_path: Option<&str>) {
    // Create Jenkins compatible output (JUnit XML format)
    let mut test_cases = Vec::new();

    for info in license_info {
        if *info.is_restrictive() {
            test_cases.push(format!(
                r#"    <testcase classname="feluda.licenses" name="{}-{}" time="0">
        <failure message="Restrictive license found" type="restrictive">
            Dependency '{}@{}' has restrictive license: {}
        </failure>
    </testcase>"#,
                info.name(),
                info.version(),
                info.name(),
                info.version(),
                info.get_license()
            ));
        } else {
            test_cases.push(format!(
                r#"    <testcase classname="feluda.licenses" name="{}-{}" time="0" />"#,
                info.name(),
                info.version()
            ));
        }
    }

    let restrictive_count = license_info.iter().filter(|i| *i.is_restrictive()).count();
    let junit_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="Feluda License Check" tests="{}" failures="{}" errors="0" skipped="0">
{}
  </testsuite>
</testsuites>"#,
        license_info.len(),
        restrictive_count,
        test_cases.join("\n")
    );

    // Output to file or stdout
    if let Some(path) = output_path {
        fs::write(path, junit_xml).expect("Failed to write Jenkins JUnit XML output file");
        println!("Jenkins JUnit XML output written to: {}", path);
    } else {
        println!("{}", junit_xml);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn get_test_data() -> Vec<LicenseInfo> {
        vec![
            LicenseInfo {
                name: "crate1".to_string(),
                version: "1.0.0".to_string(),
                license: Some("MIT".to_string()),
                is_restrictive: false,
            },
            LicenseInfo {
                name: "crate2".to_string(),
                version: "2.0.0".to_string(),
                license: Some("GPL".to_string()),
                is_restrictive: true,
            },
        ]
    }

    #[test]
    fn test_generate_report_empty_data() {
        let data = vec![];
        let result = generate_report(data, false, false, false, None, None);
        assert_eq!(result, false);
        // Expected output: 🎉 All dependencies passed the license check! No restrictive licenses found.
    }

    #[test]
    fn test_generate_report_non_strict() {
        let data = get_test_data();
        let result = generate_report(data, false, false, false, None, None);
        assert_eq!(result, true);
    }

    #[test]
    fn test_generate_report_strict() {
        let data = get_test_data();
        let result = generate_report(data, false, false, true, None, None);
        assert_eq!(result, true);
    }

    #[test]
    fn test_generate_report_json() {
        let data = get_test_data();
        let result = generate_report(data, true, false, false, None, None);
        assert_eq!(result, true);
    }

    #[test]
    fn test_generate_report_verbose() {
        let data = get_test_data();
        let result = generate_report(data, false, true, false, None, None);
        assert_eq!(result, true);
    }

    #[test]
    fn test_github_output_format() {
        let data = get_test_data();
        let temp_dir = setup();
        let output_path = temp_dir.path().join("github_output.txt");

        let _ = generate_report(
            data,
            false,
            false,
            false,
            Some(CiFormat::Github),
            Some(output_path.to_str().unwrap().to_string()),
        );

        let content = fs::read_to_string(output_path).unwrap();
        assert!(content.contains("::warning title=Restrictive License::"));
        assert!(content.contains("::notice title=License Check Summary::"));
    }

    #[test]
    fn test_jenkins_output_format() {
        let data = get_test_data();
        let temp_dir = setup();
        let output_path = temp_dir.path().join("jenkins_output.xml");

        let _ = generate_report(
            data,
            false,
            false,
            false,
            Some(CiFormat::Jenkins),
            Some(output_path.to_str().unwrap().to_string()),
        );

        let content = fs::read_to_string(output_path).unwrap();
        assert!(content.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(content.contains("<testsuites>"));
        assert!(content.contains("<failure message=\"Restrictive license found\""));
    }
}
