//! Config file path discovery helpers.

use std::fs;
use std::path::{Path, PathBuf};

pub fn resolve_config_path(
    workspace_folder: &Path,
    explicit_config: Option<&Path>,
) -> Result<PathBuf, String> {
    let config_path = if let Some(config) = explicit_config {
        expected_config_path(workspace_folder, Some(config))
    } else {
        let modern = workspace_folder
            .join(".devcontainer")
            .join("devcontainer.json");
        let legacy = workspace_folder.join(".devcontainer.json");
        if modern.is_file() {
            modern
        } else {
            legacy
        }
    };

    if !config_path.is_file() {
        return Err(format!(
            "Unable to locate a dev container config at {}",
            config_path.display()
        ));
    }

    Ok(fs::canonicalize(&config_path).unwrap_or(config_path))
}

pub fn expected_config_path(workspace_folder: &Path, explicit_config: Option<&Path>) -> PathBuf {
    if let Some(config) = explicit_config {
        if config.is_absolute() {
            config.to_path_buf()
        } else {
            workspace_folder.join(config)
        }
    } else {
        workspace_folder
            .join(".devcontainer")
            .join("devcontainer.json")
    }
}
