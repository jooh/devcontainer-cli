//! Manifest parsing and documentation helpers for collection commands.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config;

#[derive(Default)]
pub(crate) struct ManifestDocOptions {
    pub(crate) registry: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) github_owner: Option<String>,
    pub(crate) github_repo: Option<String>,
}

pub(crate) fn parse_manifest(root: &Path, manifest_name: &str) -> Result<Value, String> {
    let manifest_path = root.join(manifest_name);
    let raw = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    config::parse_jsonc_value(&raw)
}

pub(crate) fn generate_manifest_docs(
    root: &Path,
    manifest_name: &str,
    fallback_title: &str,
    options: &ManifestDocOptions,
) -> Result<PathBuf, String> {
    let manifest = parse_manifest(root, manifest_name)?;
    let readme_path = root.join("README.md");
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(fallback_title);
    let description = manifest
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("Generated documentation.");
    let mut contents = format!("# {name}\n\n{description}\n");
    if let (Some(registry), Some(namespace), Some(id)) = (
        options.registry.as_deref(),
        options.namespace.as_deref(),
        manifest.get("id").and_then(Value::as_str),
    ) {
        contents.push_str(&format!(
            "\n## OCI Reference\n\n`{registry}/{namespace}/{id}`\n"
        ));
    }
    if let (Some(owner), Some(repo)) = (
        options.github_owner.as_deref(),
        options.github_repo.as_deref(),
    ) {
        contents.push_str(&format!(
            "\n## Source Repository\n\nhttps://github.com/{owner}/{repo}\n"
        ));
    }
    fs::write(&readme_path, contents).map_err(|error| error.to_string())?;
    Ok(readme_path)
}
