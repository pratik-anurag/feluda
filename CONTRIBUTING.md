# Contributing Guide

Welcoming contributions from the community! 🙌

[![Made with Rust](https://img.shields.io/badge/Made%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)

_Minimum Supported Rust Version: `1.70.0`_

### Folder Structure:

```sh
feluda/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── cli.rs           # CLI argument handling
│   ├── config.rs        # Configuration management
│   ├── debug.rs         # Debug and logging utilities
│   ├── parser.rs        # Dependency parsing logic
│   ├── licenses.rs      # License analysis
│   ├── reporter.rs      # Output formatting and reporting
│   └── table.rs         # TUI components
├── Cargo.toml           # Project metadata
├── LICENSE              # Project license
└── README.md            # Project documentation
```

### Setting Up for Development

1. Fork the repository and clone it to your local machine:

```sh
git clone https://github.com/yourusername/feluda.git
cd feluda
```

2. Install dependencies and tools:

```sh
cargo build
```

3. Run locally

```sh
./target/debug/feluda --help
```

4. Run tests to ensure everything is working:

```sh
cargo test
```

### Debug Mode

Feluda has a comprehensive debug system that helps with troubleshooting and development. To enable debug mode, run Feluda with the `--debug` or `-d` flag:

```sh
feluda --debug
```

#### Debug Features

The debug mode provides the following features:

1. **Detailed Logging**: Log messages are printed with different levels:
   - `INFO`: General information about operations
   - `WARN`: Potential issues that don't stop execution
   - `ERROR`: Problems that caused an operation to fail
   - `TRACE`: Detailed debugging information about data structures

2. **Performance Metrics**: Debug mode automatically times key operations and reports their duration.

3. **Data Inspection**: Complex data structures are printed in debug format for inspection.

4. **Error Context**: Errors include detailed context to help identify root causes.

#### Logging in Your Code

When adding new features, include appropriate logging using the debug module:

```rust
// Import debug utilities
use crate::debug::{log, LogLevel, log_debug, log_error};

// Log informational messages
log(LogLevel::Info, "Starting important operation");

// Log warnings
log(LogLevel::Warn, "Resource XYZ not found, using default");

// Log errors with context
if let Err(err) = some_operation() {
    log_error("Failed to complete operation", &err);
}

// Log complex data structures for debugging
log_debug("Retrieved configuration", &config);

// Time operations
let result = with_debug("Complex calculation", || {
    // Your code here
    perform_complex_calculation()
});
```

#### Error Handling

Feluda uses a custom error type for consistent error handling. When adding new code, use the `FeludaError` and `FeludaResult` types:

```rust
// Return a Result with a specific error type
fn my_function() -> FeludaResult<MyType> {
    match some_operation() {
        Ok(result) => Ok(result),
        Err(err) => Err(FeludaError::Parser(format!("Operation failed: {}", err)))
    }
}
```

### Guidelines

- **Code Style**: Follow Rust's standard coding conventions.
- **Testing**: Ensure your changes are covered by unit tests.
- **Documentation**: Update relevant documentation and comments.
- **Logging**: Add appropriate debug logging for new functionality.
- **Error Handling**: Use the `FeludaError` type for consistent error reporting.

### Submitting Changes

1. Create a new branch for your feature or bugfix:

```sh
git checkout -b feature/my-new-feature
```

2. Commit your changes with a meaningful commit message:

```sh
git commit -m "Add support for XYZ feature"
```

3. Push the branch to your fork:

```sh
git push origin feature/my-new-feature
```

4. Open a pull request on GitHub.

### Reporting Issues

If you encounter a bug or have a feature request, please open an issue in the repository.

### Releasing Feluda 🚀

This is only if you've release permissions. If not, contact the maintainers to get it.

#### Setup Requirements

- Install the gh CLI:
```sh
brew install gh # macOS
sudo apt install gh # Ubuntu/Debian
```

- Authenticate the gh CLI with GitHub:
```sh
gh auth login
```

- Install jq for JSON parsing:
```sh
brew install jq # macOS
sudo apt install jq # Ubuntu/Debian
```

We'll be using justfile for next steps, so setup [just](https://github.com/casey/just) before proceeding...

#### Build the Release
```sh
just release
```

#### Test the Release Build
```sh
just test-release
```

#### Create the Package
Validate the crate before publishing
```sh
just package
```

#### Publish the Crate
```sh
just publish
```

#### Automate Everything
Run all steps (build, test, package, and publish) in one command:

```sh
just release-publish
```

#### Clean Artifacts
To clean up the build artifacts:

```sh
just clean
```

#### Login to crates.io
```sh
just login
```
