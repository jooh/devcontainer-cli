//! Workspace and config resolution helpers shared across commands.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::{self, ConfigContext};
use crate::runtime::mounts::mount_option_target;

use super::args::{parse_option_value, parse_option_values, validate_option_values};

pub(crate) fn resolve_read_configuration_path(
    args: &[String],
) -> Result<(PathBuf, PathBuf), String> {
    validate_option_values(
        args,
        &["--workspace-folder", "--config", "--override-config"],
    )?;

    let explicit_workspace = parse_option_value(args, "--workspace-folder").map(PathBuf::from);
    let explicit_config = parse_option_value(args, "--config").map(PathBuf::from);
    let override_config = resolve_override_config_path(args)?;

    let initial_workspace = explicit_workspace
        .clone()
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| "Unable to determine workspace folder".to_string())?;

    let workspace_folder = if explicit_workspace.is_some() {
        initial_workspace.clone()
    } else if let Some(explicit_config) = explicit_config.as_deref() {
        let config_path = config::expected_config_path(&initial_workspace, Some(explicit_config));
        infer_workspace_folder_from_config(&config_path)
    } else if let Some(override_config) = override_config.as_deref() {
        infer_workspace_folder_from_config(override_config)
    } else {
        initial_workspace.clone()
    };

    let config_path = if override_config.is_some() {
        let expected = config::expected_config_path(&workspace_folder, explicit_config.as_deref());
        fs::canonicalize(&expected).unwrap_or(expected)
    } else {
        config::resolve_config_path(&workspace_folder, explicit_config.as_deref())?
    };

    let resolved_workspace = if explicit_workspace.is_some() {
        fs::canonicalize(&workspace_folder).unwrap_or(workspace_folder)
    } else if explicit_config.is_some() {
        infer_workspace_folder_from_config(&config_path)
    } else if override_config.is_some() {
        infer_workspace_folder_from_config(override_config.as_deref().expect("override config"))
    } else {
        fs::canonicalize(&initial_workspace).unwrap_or(initial_workspace)
    };
    Ok((resolved_workspace, config_path))
}

fn infer_workspace_folder_from_config(config_path: &Path) -> PathBuf {
    let config_parent = config_path.parent().unwrap_or(config_path);
    let workspace = config_path
        .ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(".devcontainer"))
        .and_then(Path::parent)
        .unwrap_or(config_parent);
    fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf())
}

pub(crate) fn load_resolved_config(args: &[String]) -> Result<(PathBuf, PathBuf, Value), String> {
    let (workspace_folder, config_file) = resolve_read_configuration_path(args)?;
    let config_source = resolve_override_config_path(args)?.unwrap_or_else(|| config_file.clone());
    let raw = fs::read_to_string(&config_source).map_err(|error| error.to_string())?;
    let parsed = config::parse_jsonc_value(&raw)?;
    let base_context = ConfigContext {
        workspace_folder: workspace_folder.clone(),
        env: env::vars().collect(),
        container_workspace_folder: None,
        id_labels: id_label_map(args, &workspace_folder, &config_file),
    };
    let container_workspace_folder = parsed
        .get("workspaceFolder")
        .and_then(Value::as_str)
        .map(|value| {
            config::substitute_local_context(&Value::String(value.to_string()), &base_context)
        })
        .and_then(|value| value.as_str().map(str::to_string))
        .or_else(|| {
            parsed
                .get("workspaceMount")
                .and_then(Value::as_str)
                .and_then(|mount| {
                    let substituted = config::substitute_local_context(
                        &Value::String(mount.to_string()),
                        &base_context,
                    );
                    substituted.as_str().and_then(mount_option_target)
                })
        })
        .or_else(|| {
            Some(
                crate::runtime::context::derived_workspace_mount(&workspace_folder, args)
                    .map(|derived| derived.remote_workspace_folder)
                    .unwrap_or_else(|| {
                        crate::runtime::context::default_remote_workspace_folder(Some(
                            &workspace_folder,
                        ))
                    }),
            )
        });
    let substituted = config::substitute_local_context(
        &parsed,
        &ConfigContext {
            workspace_folder: base_context.workspace_folder.clone(),
            env: base_context.env,
            container_workspace_folder,
            id_labels: base_context.id_labels,
        },
    );
    Ok((workspace_folder, config_file, substituted))
}

pub(crate) fn resolve_override_config_path(args: &[String]) -> Result<Option<PathBuf>, String> {
    let Some(path) = parse_option_value(args, "--override-config") else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    let resolved = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    if !resolved.is_file() {
        return Err(format!(
            "Unable to locate an override dev container config at {}",
            resolved.display()
        ));
    }
    Ok(Some(fs::canonicalize(&resolved).unwrap_or(resolved)))
}

pub(crate) fn id_label_map(
    args: &[String],
    workspace_folder: &Path,
    config_file: &Path,
) -> HashMap<String, String> {
    let mut labels = parse_option_values(args, "--id-label")
        .into_iter()
        .filter_map(|entry| {
            entry
                .split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
        })
        .collect::<HashMap<_, _>>();
    if labels.is_empty() {
        labels.insert(
            "devcontainer.local_folder".to_string(),
            workspace_folder.display().to_string(),
        );
        labels.insert(
            "devcontainer.config_file".to_string(),
            config_file.display().to_string(),
        );
    }
    labels
}
