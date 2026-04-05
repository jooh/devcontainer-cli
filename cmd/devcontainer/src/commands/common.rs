use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::config::{self, ConfigContext};
use crate::process_runner::{self, ProcessRequest};

pub(crate) fn parse_option_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == option)
        .map(|window| window[1].clone())
}

pub(crate) fn parse_option_values(args: &[String], option: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == option && index + 1 < args.len() {
            values.push(args[index + 1].clone());
            index += 2;
        } else {
            index += 1;
        }
    }
    values
}

pub(crate) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

pub(crate) fn parse_mounts(args: &[String]) -> Vec<Value> {
    parse_option_values(args, "--mount")
        .into_iter()
        .map(Value::String)
        .collect()
}

pub(crate) fn parse_remote_env(args: &[String]) -> Map<String, Value> {
    parse_option_values(args, "--remote-env")
        .into_iter()
        .filter_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            Some((name.to_string(), Value::String(value.to_string())))
        })
        .collect()
}

pub(crate) fn remote_env_overrides(args: &[String]) -> HashMap<String, String> {
    parse_remote_env(args)
        .into_iter()
        .filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string())))
        .collect()
}

pub(crate) fn resolve_read_configuration_path(
    args: &[String],
) -> Result<(PathBuf, PathBuf), String> {
    let workspace_folder = parse_option_value(args, "--workspace-folder")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| "Unable to determine workspace folder".to_string())?;

    let explicit_config = parse_option_value(args, "--config").map(PathBuf::from);
    let config_path = config::resolve_config_path(&workspace_folder, explicit_config.as_deref())?;

    let resolved_workspace = fs::canonicalize(&workspace_folder).unwrap_or(workspace_folder);
    Ok((resolved_workspace, config_path))
}

pub(crate) fn load_resolved_config(args: &[String]) -> Result<(PathBuf, PathBuf, Value), String> {
    let (workspace_folder, config_file) = resolve_read_configuration_path(args)?;
    let raw = fs::read_to_string(&config_file).map_err(|error| error.to_string())?;
    let parsed = config::parse_jsonc_value(&raw)?;
    let substituted = config::substitute_local_context(
        &parsed,
        &ConfigContext {
            workspace_folder: workspace_folder.clone(),
            env: env::vars().collect(),
        },
    );
    Ok((workspace_folder, config_file, substituted))
}

pub(crate) fn lifecycle_commands(configuration: &Value) -> Vec<Value> {
    [
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ]
    .iter()
    .filter_map(|key| {
        configuration
            .get(*key)
            .map(|value| json!({ "name": key, "value": value }))
    })
    .collect()
}

pub(crate) fn parse_manifest(root: &Path, manifest_name: &str) -> Result<Value, String> {
    let manifest_path = root.join(manifest_name);
    let raw = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    config::parse_jsonc_value(&raw)
}

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
    })?;

    if result.status_code != 0 {
        return Err(result.stderr);
    }

    Ok(archive_path)
}

pub(crate) fn generate_manifest_docs(
    root: &Path,
    manifest_name: &str,
    fallback_title: &str,
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
    fs::write(&readme_path, format!("# {name}\n\n{description}\n"))
        .map_err(|error| error.to_string())?;
    Ok(readme_path)
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
