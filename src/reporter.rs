use crate::cli::CiFormat;
use crate::licenses::LicenseInfo;
use colored::*;
use std::collections::HashMap;
use std::fs;

struct TableFormatter {
    column_widths: Vec<usize>,
    headers: Vec<String>,
}

impl TableFormatter {
    fn new(headers: Vec<String>) -> Self {
        let column_widths = headers.iter().map(|h| h.len()).collect();
        Self {
            column_widths,
            headers,
        }
    }

    fn add_row(&mut self, row: &[String]) {
        for (i, item) in row.iter().enumerate() {
            if i < self.column_widths.len() {
                self.column_widths[i] = self.column_widths[i].max(item.len());
            }
        }
    }

    fn render_header(&self) -> String {
        let header_row = self
            .headers
            .iter()
            .enumerate()
            .map(|(i, header)| format!("{:width$}", header, width = self.column_widths[i]))
            .collect::<Vec<_>>()
            .join(" │ ");

        let total_width =
            self.column_widths.iter().sum::<usize>() + (3 * self.column_widths.len()) - 1;

        format!(
            "┌{}┐\n│ {} │\n├{}┤",
            "─".repeat(total_width),
            header_row.bold().blue(),
            "─".repeat(total_width)
        )
    }

    fn render_row(&self, row: &[String], is_restrictive: bool) -> String {
        let formatted_row = row
            .iter()
            .enumerate()
            .map(|(i, item)| {
                if i < self.column_widths.len() {
                    format!("{:width$}", item, width = self.column_widths[i])
                } else {
                    item.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" │ ");

        if is_restrictive {
            format!("│ {} │", formatted_row.red().bold())
        } else {
            format!("│ {} │", formatted_row.green())
        }
    }

    fn render_footer(&self) -> String {
        let footer_width =
            self.column_widths.iter().sum::<usize>() + (3 * self.column_widths.len()) - 1;
        format!("└{}┘", "─".repeat(footer_width))
    }
}

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
            "\n{}\n",
            "🎉 All dependencies passed the license check! No restrictive licenses found."
                .green()
                .bold()
        );
        return false;
    }

    if let Some(format) = ci_format {
        match format {
            CiFormat::Github => output_github_format(&filtered_data, output_file.as_deref()),
            CiFormat::Jenkins => output_jenkins_format(&filtered_data, output_file.as_deref()),
        }
    } else if json {
        // JSON output
        let json_output =
            serde_json::to_string_pretty(&filtered_data).expect("Failed to serialize data");
        println!("{}", json_output);
    } else {
        if verbose {
            print_verbose_table(&filtered_data, strict);
        } else {
            print_summary_table(&filtered_data, total_packages, strict);
        }
    }

    has_restrictive
}

fn print_verbose_table(license_info: &[LicenseInfo], strict: bool) {
    let headers = vec![
        "Name".to_string(),
        "Version".to_string(),
        "License".to_string(),
        "Restrictive".to_string(),
    ];

    let mut formatter = TableFormatter::new(headers);

    let rows: Vec<_> = license_info
        .iter()
        .map(|info| {
            vec![
                info.name().to_string(),
                info.version().to_string(),
                info.get_license(),
                info.is_restrictive().to_string(),
            ]
        })
        .collect();

    for row in &rows {
        formatter.add_row(row);
    }

    println!("\n{}", formatter.render_header());

    for (i, row) in rows.iter().enumerate() {
        let is_restrictive = *license_info[i].is_restrictive();
        println!("{}", formatter.render_row(row, is_restrictive));
    }

    println!("{}\n", formatter.render_footer());

    if !strict {
        print_summary_footer(license_info);
    }
}

fn print_summary_table(license_info: &[LicenseInfo], total_packages: usize, strict: bool) {
    if strict {
        print_restrictive_licenses_table(&license_info.iter().collect::<Vec<_>>());
        return;
    }

    let mut license_count: HashMap<String, Vec<String>> = HashMap::new();
    let mut restrictive_licenses: Vec<&LicenseInfo> = Vec::new();

    for info in license_info {
        let license = info.get_license();

        if *info.is_restrictive() {
            restrictive_licenses.push(info);
        } else {
            license_count
                .entry(license)
                .or_default()
                .push(info.name().to_string());
        }
    }

    // License summary
    let headers = vec!["License Type".to_string(), "Count".to_string()];

    let mut formatter = TableFormatter::new(headers);

    let mut rows: Vec<Vec<String>> = license_count
        .iter()
        .map(|(license, deps)| vec![license.clone(), deps.len().to_string()])
        .collect();

    for row in &rows {
        formatter.add_row(row);
    }

    println!(
        "\n{} {}\n",
        "🔍".bold(),
        "License Summary".bold().underline()
    );

    println!("{}", formatter.render_header());

    rows.sort_by(|a, b| a[0].cmp(&b[0]));

    for row in &rows {
        println!("{}", formatter.render_row(row, false));
    }

    println!("{}", formatter.render_footer());

    println!(
        "\n{} {}",
        "📦".bold(),
        format!("Total dependencies scanned: {}", total_packages).bold()
    );

    if !restrictive_licenses.is_empty() {
        print_restrictive_licenses_table(&restrictive_licenses);
    } else {
        println!(
            "\n{}\n",
            "✅ No restrictive licenses found! 🎉".green().bold()
        );
    }
}

fn print_restrictive_licenses_table(restrictive_licenses: &[&LicenseInfo]) {
    println!(
        "\n{} {}\n",
        "⚠️".bold(),
        "Warning: Restrictive licenses found!".red().bold()
    );

    let headers = vec![
        "Package".to_string(),
        "Version".to_string(),
        "License".to_string(),
    ];

    let mut formatter = TableFormatter::new(headers);

    let rows: Vec<_> = restrictive_licenses
        .iter()
        .map(|info| {
            vec![
                info.name().to_string(),
                info.version().to_string(),
                info.get_license(),
            ]
        })
        .collect();

    for row in &rows {
        formatter.add_row(row);
    }

    println!("{}", formatter.render_header());

    for row in &rows {
        println!("{}", formatter.render_row(row, true));
    }

    println!("{}\n", formatter.render_footer());
}

fn print_summary_footer(license_info: &[LicenseInfo]) {
    let total = license_info.len();
    let restrictive_count = license_info.iter().filter(|i| *i.is_restrictive()).count();
    let permissive_count = total - restrictive_count;

    println!("{}", "🔍 License Summary:".bold());
    println!(
        "  • {} {}",
        permissive_count.to_string().green().bold(),
        "permissive licenses".green()
    );
    println!(
        "  • {} {}",
        restrictive_count.to_string().red().bold(),
        "restrictive licenses".red()
    );
    println!("  • {} total dependencies", total);

    if restrictive_count > 0 {
        println!("\n{} {}: Review these dependencies for compliance with your project's licensing requirements.",
            "⚠️".yellow().bold(),
            "Recommendation".yellow().bold()
        );
    } else {
        println!(
            "\n{} {}: All dependencies have permissive licenses compatible with most projects.",
            "✅".green().bold(),
            "Status".green().bold()
        );
    }

    println!();
}

fn output_github_format(license_info: &[LicenseInfo], output_path: Option<&str>) {
    // GitHub Actions compatible output
    let mut output = String::new();

    // GitHub Actions workflow commands format
    for info in license_info {
        if *info.is_restrictive() {
            output.push_str(&format!(
                "::warning title=Restrictive License::Dependency '{}@{}' has restrictive license: {}\n",
                info.name(),
                info.version(),
                info.get_license()
            ));
        }
    }

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
    // Jenkins compatible output (JUnit XML format)
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
