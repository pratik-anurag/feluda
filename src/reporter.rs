use crate::licenses::LicenseInfo;

pub fn generate_report(data: Vec<LicenseInfo>, json: bool, verbose: bool) {
    if json {
        let json_output = serde_json::to_string_pretty(&data).expect("Failed to serialize data");
        println!("{}", json_output);
    } else {
        for info in data {
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
