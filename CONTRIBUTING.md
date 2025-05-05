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
│   ├── parser.rs        # Dependency parsing logic
│   ├── licenses.rs      # License analysis
│   └── reporter.rs      # Output formatting and reporting
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

### Guidelines

- **Code Style**: Follow Rust's standard coding conventions.
- **Testing**: Ensure your changes are covered by unit tests.
- **Documentation**: Update relevant documentation and comments.

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

### Debug Mode

You can now add `cli::DEBUG_MODE.load(Ordering::Relaxed)` wherever you need to add debug logs.
