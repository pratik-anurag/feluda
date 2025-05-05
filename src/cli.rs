use clap::{ArgGroup, Parser, ValueEnum};
use spinners::{Spinner, Spinners};
use std::io::{self, Write};

// Import from the debug module instead of defining here
use crate::debug::{is_debug_mode, log, LogLevel};

/// CI output format options
#[derive(ValueEnum, Clone, Debug)]
pub enum CiFormat {
    /// GitHub Actions compatible format
    Github,
    /// Jenkins compatible format (JUnit XML)
    Jenkins,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(group(ArgGroup::new("output").args(["json"])))]
pub struct Cli {
    /// Path to the local project directory
    #[arg(short, long, default_value = "./")]
    pub path: String,

    /// Output in JSON format
    #[arg(long, short, group = "output")]
    /// This will override the default output format
    /// and will not show the TUI table.
    /// This is useful for CI/CD pipelines.
    pub json: bool,

    /// Enable verbose output
    #[arg(long)]
    pub verbose: bool,

    /// Show only restrictive dependencies in strict mode
    #[arg(long, short)]
    pub strict: bool,

    /// Enable TUI table
    #[arg(long, short)]
    pub gui: bool,

    /// Enable debug mode
    #[arg(long, short)]
    pub debug: bool,

    /// Specify the language to scan
    #[arg(long, short)]
    pub language: Option<String>,

    /// Output format for CI systems (github, jenkins)
    #[arg(long, value_enum)]
    pub ci_format: Option<CiFormat>,

    /// Path to write the CI report file
    #[arg(long)]
    pub output_file: Option<String>,

    /// Fail with non-zero exit code when restrictive licenses are found
    #[arg(long)]
    pub fail_on_restrictive: bool,
}

pub fn clear_last_line() {
    print!("\x1b[1A\x1b[2K");
    io::stdout().flush().unwrap();
}

pub fn with_spinner<F, T>(message: &str, f: F) -> T
where
    F: FnOnce() -> T,
{
    if is_debug_mode() {
        log(LogLevel::Info, &format!("Operation: {}", message));
        let start = std::time::Instant::now();
        let result = f();
        let duration = start.elapsed();
        log(LogLevel::Info, &format!("Completed in {:?}", duration));
        result
    } else {
        let mut sp = Spinner::new(Spinners::Dots10, message.into());
        let result = f();
        sp.stop();
        clear_last_line();
        result
    }
}
