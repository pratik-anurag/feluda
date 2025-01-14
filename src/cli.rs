use clap::{Parser, ArgGroup};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(group(ArgGroup::new("output").args(["json"])))]
pub struct Cli {
    /// Path to the Cargo.toml file
    #[arg(short, long, default_value = "./Cargo.toml")]
    pub path: String,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,

    /// Enable verbose output
    #[arg(long)]
    pub verbose: bool,

    /// Show only restrictive dependencies in strict mode
    #[arg(long)]
    pub strict: bool,
}
