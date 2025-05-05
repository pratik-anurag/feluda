mod cli;
mod config;
mod debug;
mod licenses;
mod parser;
mod reporter;
mod table;

use clap::Parser;
use cli::Cli;
use debug::{log, log_debug, set_debug_mode, FeludaError, FeludaResult, LogLevel};
use parser::parse_root;
use reporter::generate_report;
use std::process;
use table::App;

fn main() {
    match run() {
        Ok(_) => {}
        Err(e) => {
            e.log();
            process::exit(1);
        }
    }
}

fn run() -> FeludaResult<()> {
    let args = Cli::parse();

    // Debug mode
    if args.debug {
        set_debug_mode(true);
    }

    log(
        LogLevel::Info,
        &format!("Starting Feluda with args: {:?}", args),
    );

    // Parse project dependencies
    log(
        LogLevel::Info,
        &format!("Parsing dependencies in path: {}", args.path),
    );
    let analyzed_data = parse_root(&args.path, args.language.as_deref())
        .map_err(|e| FeludaError::Parser(format!("Failed to parse dependencies: {}", e)))?;

    log_debug("Analyzed dependencies", &analyzed_data);

    // Either run the GUI or generate a report
    if args.gui {
        log(LogLevel::Info, "Starting TUI mode");

        // Initialize the terminal
        color_eyre::install()
            .map_err(|e| FeludaError::Unknown(format!("Failed to initialize color_eyre: {}", e)))?;

        let terminal = ratatui::init();
        log(LogLevel::Info, "Terminal initialized for TUI");

        // TUI app
        let app_result = App::new(analyzed_data).run(terminal);
        ratatui::restore();

        // Handle any errors from the TUI
        app_result.map_err(|e| FeludaError::Unknown(format!("TUI error: {}", e)))?;

        log(LogLevel::Info, "TUI session completed successfully");
    } else {
        log(LogLevel::Info, "Generating dependency report");

        // Generate a report based on the analyzed data
        let has_restrictive = generate_report(
            analyzed_data,
            args.json,
            args.verbose,
            args.strict,
            args.ci_format,
            args.output_file.clone(),
        );

        log(
            LogLevel::Info,
            &format!("Report generated, has_restrictive: {}", has_restrictive),
        );

        // Exit with non-zero code if requested and restrictive licenses found
        if args.fail_on_restrictive && has_restrictive {
            log(
                LogLevel::Warn,
                "Exiting with non-zero status due to restrictive licenses",
            );
            process::exit(1);
        }
    }

    log(LogLevel::Info, "Feluda completed successfully");
    Ok(())
}
