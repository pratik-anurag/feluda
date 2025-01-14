use crate::licenses::LicenseInfo;

pub fn generate_report(data: Vec<LicenseInfo>, json: bool, verbose: bool, strict: bool) {
    let filtered_data: Vec<LicenseInfo> = if strict {
        data.into_iter().filter(|info| info.is_restrictive).collect()
    } else {
        data
    };

    if filtered_data.is_empty() {
        println!("\n🎉 All dependencies passed the license check! No restrictive licenses found.\n");
        return;
    }

    if json {
        let json_output = serde_json::to_string_pretty(&filtered_data).expect("Failed to serialize data");
        println!("{}", json_output);
    } else {
        for info in filtered_data {
            if verbose {
                println!(
                    "Name: {}, Version: {}, License: {:?}, Restrictive: {}",
                    info.name, info.version, info.get_license(), info.is_restrictive
                );
            } else {
                println!("{}@{} - {:?}", info.name, info.version, info.get_license());
            }
        }
    }
}
