# Variables
CRATE_NAME := "feluda"
VERSION := `cargo pkgid | cut -d# -f2 | cut -d: -f2`
GITHUB_REPO := "anistark/feluda"

# Build the crate
build:
    @echo "🚀 Building release version..."
    cargo build --release

# Create the crate package (to validate before publishing)
package:
    @echo "📦 Creating package for validation..."
    cargo package

# Test the release build
test-release:
    @echo "🧪 Testing the release build..."
    cargo test --release

# Create a release on GitHub
gh-release:
    @echo "📢 Creating GitHub release for version {{VERSION}}"
    gh release create {{VERSION}} --title "{{CRATE_NAME}} {{VERSION}}" --notes "Release {{VERSION}}"

# Publish the crate to crates.io
publish:
    just build
    just test-release
    just package
    cargo publish
    just gh-release

# Clean up the build artifacts
clean:
    @echo "🧹 Cleaning up build artifacts..."
    cargo clean

# Login to crates.io
login:
    @echo "🔑 Logging in to crates.io..."
    cargo login
