use clap::{ArgGroup, Parser};
use spinners::{Spinner, Spinners};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

pub static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_debug_mode(debug: bool) {
    DEBUG_MODE.store(debug, Ordering::Relaxed);
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
}

pub fn clear_last_line() {
    print!("\x1b[1A\x1b[2K");
    io::stdout().flush().unwrap();
}

pub fn with_spinner<F, T>(message: &str, f: F) -> T
where
    F: FnOnce() -> T,
{
    if DEBUG_MODE.load(Ordering::Relaxed) {
        f()
    } else {
        let mut sp = Spinner::new(Spinners::Dots10, message.into());
        let result = f();
        sp.stop();
        clear_last_line();
        result
    }
}
