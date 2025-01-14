# Feluda

Feluda is a Rust-based command-line tool that analyzes the dependencies of a Rust project, notes down their licenses, and flags any permissions that restrict personal or commercial usage. Currently, Feluda supports analyzing `Cargo.toml` files and provides a comprehensive summary of the project's dependencies and their license details.

## Features

- Parse `Cargo.toml` to identify dependencies and their licenses.
- Classify licenses into permissive, restrictive, or unknown categories.
- Flag dependencies with licenses that may restrict personal or commercial use.
- Output results in plain text or JSON format.

## Installation

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) installed on your system.

If you already had it, make sure it's up-to-date and update if needed.

### Clone and Build

```sh
# Clone the repository
git clone https://github.com/anistark/feluda.git
cd feluda

# Build the project
cargo build --release

# Add Feluda to your PATH (optional)
export PATH="$PWD/target/release:$PATH"
```

## Usage

### Basic Usage

Run the tool in the directory containing your Cargo.toml file:

```sh
feluda
```

### Specify a Path to Cargo.toml

```sh
feluda --path /path/to/Cargo.toml
```

### Output Format

- Default: Plain text.
- JSON: Use the `--json` flag for JSON output.

```sh
feluda --json
```

Sample Output for a sample cargo.toml file containing `serde` and `tokio` dependencies:

```json
[
  {
    "name": "serde",
    "version": "1.0.151",
    "license": "MIT",
    "is_restrictive": false
  },
  {
    "name": "tokio",
    "version": "1.0.2",
    "license": "MIT",
    "is_restrictive": false
  }
]
```

### Verbose Mode

For detailed information about each dependency:

```sh
feluda --verbose
```

### Strict Mode

In case you strictly need only the restrictive dependencies:

```sh
feluda --strict
```

---

## Contributing

Welcoming contributions from the community! 

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

3. Run tests to ensure everything is working:

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

---

## License

Feluda is licensed under the [MIT License with Commons Clause](./LICENSE).


_Happy coding with Feluda!_ 🚀
