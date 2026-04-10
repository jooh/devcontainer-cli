//! Filesystem helpers shared across command implementations.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::process_runner::{self, ProcessLogLevel, ProcessRequest};

use super::manifest::parse_manifest;

pub(crate) fn package_collection_target(
    target: &Path,
    manifest_name: &str,
    prefix: &str,
) -> Result<PathBuf, String> {
    let _ = parse_manifest(target, manifest_name)?;
    let archive_name = format!(
        "{}-{}.tgz",
        prefix,
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(prefix)
    );
    let archive_path = target.parent().unwrap_or(target).join(archive_name);

    let result = process_runner::run_process(&ProcessRequest {
        program: "tar".to_string(),
        args: vec![
            "-czf".to_string(),
            archive_path.display().to_string(),
            "-C".to_string(),
            target.display().to_string(),
            ".".to_string(),
        ],
        cwd: None,
        env: HashMap::new(),
        log_level: ProcessLogLevel::Info,
    })?;

    if result.status_code != 0 {
        return Err(result.stderr);
    }

    Ok(archive_path)
}

pub(crate) fn copy_directory_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry_path.is_dir() {
            copy_directory_recursive(&entry_path, &destination_path)?;
        } else {
            fs::copy(&entry_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
