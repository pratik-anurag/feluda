use cargo_metadata::{MetadataCommand, Package};
use std::path::Path;

pub fn parse_dependencies(cargo_toml_path: &str) -> Vec<Package> {
    let metadata = MetadataCommand::new()
        .manifest_path(Path::new(cargo_toml_path))
        .exec()
        .expect("Failed to fetch cargo metadata");

    metadata.packages
}
