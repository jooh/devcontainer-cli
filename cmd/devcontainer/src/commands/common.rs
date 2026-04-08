use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::config::{self, ConfigContext};
use crate::process_runner::{self, ProcessRequest};

#[derive(Default)]
pub(crate) struct ManifestDocOptions {
    pub(crate) registry: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) github_owner: Option<String>,
    pub(crate) github_repo: Option<String>,
}

pub(crate) fn parse_option_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == option)
        .map(|window| window[1].clone())
}

pub(crate) fn parse_bool_option(args: &[String], option: &str, default: bool) -> bool {
    let Some(index) = args.iter().position(|arg| arg == option) else {
        return default;
    };
    match args.get(index + 1).map(String::as_str) {
        Some("false" | "0" | "no" | "off") => false,
        Some("true" | "1" | "yes" | "on") => true,
        Some(next) if next.starts_with("--") => true,
        Some(_) => true,
        None => true,
    }
}

pub(crate) fn validate_option_values(args: &[String], options: &[&str]) -> Result<(), String> {
    for (index, arg) in args.iter().enumerate() {
        if options.contains(&arg.as_str())
            && args
                .get(index + 1)
                .is_none_or(|next| next.starts_with("--"))
        {
            return Err(format!("Missing value for option: {arg}"));
        }
    }

    Ok(())
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

pub(crate) fn parse_json_string_array_option(
    args: &[String],
    option: &str,
) -> Result<Vec<String>, String> {
    let Some(value) = parse_option_value(args, option) else {
        return Ok(Vec::new());
    };
    let parsed = config::parse_jsonc_value(&value)?;
    let values = parsed
        .as_array()
        .ok_or_else(|| format!("{option} must be a JSON array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{option} entries must be strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(values)
}

pub(crate) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
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

pub(crate) fn secrets_env(args: &[String]) -> Result<HashMap<String, String>, String> {
    let Some(path) = parse_option_value(args, "--secrets-file") else {
        return Ok(HashMap::new());
    };
    let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let parsed = config::parse_jsonc_value(&raw)?;
    let entries = parsed
        .as_object()
        .ok_or_else(|| "--secrets-file must point to a JSON object".to_string())?;
    Ok(entries
        .iter()
        .filter_map(|(key, value)| match value {
            Value::Null => None,
            Value::Bool(boolean) => Some((key.clone(), boolean.to_string())),
            Value::Number(number) => Some((key.clone(), number.to_string())),
            Value::String(text) => Some((key.clone(), text.clone())),
            _ => Some((key.clone(), value.to_string())),
        })
        .collect())
}

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
                    substituted
                        .as_str()
                        .and_then(crate::runtime::metadata::mount_option_target)
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
