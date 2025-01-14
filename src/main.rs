mod cli;
mod parser;
mod licenses;
mod reporter;

use clap::Parser;
use cli::Cli;
use parser::parse_dependencies;
use licenses::analyze_licenses;
use reporter::generate_report;

fn main() {
    let args = Cli::parse();
    let dependencies = parse_dependencies(&args.path);
    let analyzed_data = analyze_licenses(dependencies);
    generate_report(analyzed_data, args.json, args.verbose);
}
