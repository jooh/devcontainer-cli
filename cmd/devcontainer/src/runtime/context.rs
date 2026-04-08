use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};

use crate::commands::common;
use crate::config::{self, ConfigContext};

use super::compose;
use super::container;
use super::engine;
use super::metadata::{merged_container_metadata, mount_option_target};

pub(crate) struct ResolvedConfig {
    pub(crate) workspace_folder: PathBuf,
    pub(crate) config_file: PathBuf,
    pub(crate) configuration: Value,
}

struct InspectedContainerContext {
    configuration: Value,
    local_workspace_folder: Option<PathBuf>,
    remote_workspace_folder: Option<String>,
}

pub(crate) struct ExistingContainerContext {
    pub(crate) container_id: String,
    pub(crate) configuration: Value,
    pub(crate) remote_workspace_folder: String,
}

pub(crate) struct DerivedWorkspaceMount {
    pub(crate) host_mount_folder: PathBuf,
    pub(crate) container_mount_folder: String,
    pub(crate) remote_workspace_folder: String,
}

pub(crate) fn load_required_config(args: &[String]) -> Result<ResolvedConfig, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    Ok(ResolvedConfig {
        workspace_folder,
        config_file,
        configuration,
    })
}

pub(crate) fn load_optional_config(args: &[String]) -> Result<Option<ResolvedConfig>, String> {
    let explicit_config = common::parse_option_value(args, "--config");
    match load_required_config(args) {
        Ok(config) => Ok(Some(config)),
        Err(error)
            if explicit_config.is_none()
                && error.starts_with("Unable to locate a dev container config at ") =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn resolve_existing_container_context(
    args: &[String],
) -> Result<ExistingContainerContext, String> {
    let resolved = load_optional_config(args)?;
    let explicit_container_id = common::parse_option_value(args, "--container-id");
    if let Some(resolved) = &resolved {
        if explicit_container_id.is_none() && compose::uses_compose_config(&resolved.configuration)
        {
            let container_id = compose::resolve_container_id(resolved, args)?
                .ok_or_else(|| "Dev container not found.".to_string())?;
            return Ok(ExistingContainerContext {
                container_id,
                configuration: resolved.configuration.clone(),
                remote_workspace_folder: remote_workspace_folder_for_args(resolved, args),
            });
        }
    }
    let workspace_folder = if let Some(resolved) = &resolved {
        Some(resolved.workspace_folder.clone())
    } else {
        workspace_folder_from_args(args)?
    };
    let container_id = container::resolve_target_container(
        args,
        resolved
            .as_ref()
            .map(|value| value.workspace_folder.as_path())
            .or(workspace_folder.as_deref()),
        resolved.as_ref().map(|value| value.config_file.as_path()),
    )?;
    let inspected = if resolved.is_none() {
        Some(inspect_container_context(args, &container_id)?)
    } else {
        None
    };
    let configuration = resolved
        .as_ref()
        .map(|value| value.configuration.clone())
        .or_else(|| inspected.as_ref().map(|value| value.configuration.clone()))
        .unwrap_or_else(|| Value::Object(Map::new()));
    let remote_workspace_folder = resolved
        .as_ref()
        .map(|resolved| remote_workspace_folder_for_args(resolved, args))
        .or_else(|| {
            inspected
                .as_ref()
                .and_then(|value| value.remote_workspace_folder.clone())
        })
        .unwrap_or_else(|| {
            default_remote_workspace_folder(
                inspected
                    .as_ref()
                    .and_then(|value| value.local_workspace_folder.as_deref())
                    .or(workspace_folder.as_deref()),
            )
        });

    Ok(ExistingContainerContext {
        container_id,
        configuration,
        remote_workspace_folder,
    })
}

pub(crate) fn remote_user(configuration: &Value) -> String {
    configured_user(configuration).unwrap_or("root").to_string()
}

pub(crate) fn configured_user(configuration: &Value) -> Option<&str> {
    configuration
        .get("remoteUser")
        .or_else(|| configuration.get("containerUser"))
        .and_then(Value::as_str)
}

pub(crate) fn combined_remote_env(
    args: &[String],
    configuration: Option<&Value>,
) -> Result<HashMap<String, String>, String> {
    let mut remote_env = HashMap::new();
    if let Some(config_env) = configuration
        .and_then(|value| value.get("remoteEnv"))
        .and_then(Value::as_object)
    {
        for (key, value) in config_env {
            if let Some(value) = value.as_str() {
                remote_env.insert(key.clone(), value.to_string());
            }
        }
    }
    remote_env.extend(common::secrets_env(args)?);
    remote_env.extend(common::remote_env_overrides(args));
    Ok(remote_env)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn remote_workspace_folder(resolved: &ResolvedConfig) -> String {
    resolved
        .configuration
        .get("workspaceFolder")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            resolved
                .configuration
                .get("workspaceMount")
                .and_then(Value::as_str)
                .and_then(mount_option_target)
        })
        .unwrap_or_else(|| {
            derived_workspace_mount(&resolved.workspace_folder, &[])
                .map(|derived| derived.remote_workspace_folder)
                .unwrap_or_else(|| {
                    default_remote_workspace_folder(Some(&resolved.workspace_folder))
                })
        })
}

#[allow(dead_code)]
pub(crate) fn workspace_mount(resolved: &ResolvedConfig, remote_workspace_folder: &str) -> String {
    resolved
        .configuration
        .get("workspaceMount")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            default_workspace_mount(&resolved.workspace_folder, remote_workspace_folder, &[])
        })
}

pub(crate) fn remote_workspace_folder_for_args(
    resolved: &ResolvedConfig,
    args: &[String],
) -> String {
    resolved
        .configuration
        .get("workspaceFolder")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            resolved
                .configuration
                .get("workspaceMount")
                .and_then(Value::as_str)
                .and_then(mount_option_target)
        })
        .unwrap_or_else(|| {
            derived_workspace_mount(&resolved.workspace_folder, args)
                .map(|derived| derived.remote_workspace_folder)
                .unwrap_or_else(|| {
                    default_remote_workspace_folder(Some(&resolved.workspace_folder))
                })
        })
}

pub(crate) fn workspace_mount_for_args(
    resolved: &ResolvedConfig,
    remote_workspace_folder: &str,
    args: &[String],
) -> String {
    resolved
        .configuration
        .get("workspaceMount")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            default_workspace_mount(&resolved.workspace_folder, remote_workspace_folder, args)
        })
}

pub(crate) fn default_remote_workspace_folder(workspace_folder: Option<&Path>) -> String {
    let basename = workspace_folder
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .unwrap_or("workspace");
    format!("/workspaces/{basename}")
}

pub(crate) fn derived_workspace_mount(
    workspace_folder: &Path,
    args: &[String],
) -> Option<DerivedWorkspaceMount> {
    let host_mount_folder = if common::parse_bool_option(args, "--mount-workspace-git-root", true) {
        find_git_root_folder(workspace_folder).unwrap_or_else(|| workspace_folder.to_path_buf())
    } else {
        workspace_folder.to_path_buf()
    };
    let container_mount_folder = format!(
        "/workspaces/{}",
        host_mount_folder
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("workspace")
    );
    let relative = workspace_folder.strip_prefix(&host_mount_folder).ok();
    let remote_workspace_folder = relative
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| {
            let suffix = path
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            format!("{container_mount_folder}/{suffix}")
        })
        .unwrap_or_else(|| container_mount_folder.clone());
    Some(DerivedWorkspaceMount {
        host_mount_folder,
        container_mount_folder,
        remote_workspace_folder,
    })
}

fn default_workspace_mount(
    workspace_folder: &Path,
    remote_workspace_folder: &str,
    args: &[String],
) -> String {
    let Some(derived) = derived_workspace_mount(workspace_folder, args) else {
        return format!(
            "type=bind,source={},target={remote_workspace_folder}",
            workspace_folder.display()
        );
    };
    let mut mount = format!(
        "type=bind,source={},target={}",
        derived.host_mount_folder.display(),
        derived.container_mount_folder
    );
    if std::env::consts::OS != "linux" {
        if let Some(consistency) = common::parse_option_value(args, "--workspace-mount-consistency")
        {
            mount.push_str(&format!(",consistency={consistency}"));
        }
    }
    mount
}

fn find_git_root_folder(workspace_folder: &Path) -> Option<PathBuf> {
    let git_output = Command::new("git")
        .args(["rev-parse", "--show-cdup"])
        .current_dir(workspace_folder)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let cdup = String::from_utf8_lossy(&git_output.stdout)
        .trim()
        .to_string();
    if cdup.is_empty() {
        return Some(workspace_folder.to_path_buf());
    }
    fs::canonicalize(workspace_folder.join(&cdup))
        .ok()
        .or_else(|| {
            let candidate = workspace_folder.join(&cdup);
            candidate.exists().then_some(candidate)
        })
}

fn inspect_container_context(
    args: &[String],
    container_id: &str,
) -> Result<InspectedContainerContext, String> {
    let result = engine::run_engine(args, vec!["inspect".to_string(), container_id.to_string()])?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    let inspected: Value = serde_json::from_str(&result.stdout)
        .map_err(|error| format!("Invalid inspect JSON: {error}"))?;
    let details = inspected
        .as_array()
        .and_then(|entries| entries.first())
        .ok_or_else(|| "Container engine did not return inspect details".to_string())?;
    let labels = details
        .get("Config")
        .and_then(|value| value.get("Labels"))
        .and_then(Value::as_object);
    let local_workspace_folder = labels
        .and_then(|entries| entries.get("devcontainer.local_folder"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let mut configuration = merged_container_metadata(
        labels
            .and_then(|entries| entries.get("devcontainer.metadata"))
            .and_then(Value::as_str),
    );
    if let Some(workspace_folder) = &local_workspace_folder {
        configuration = config::substitute_local_context(
            &configuration,
            &ConfigContext {
                workspace_folder: workspace_folder.clone(),
                env: env::vars().collect(),
                container_workspace_folder: None,
                id_labels: HashMap::new(),
            },
        );
    }
    if configured_user(&configuration).is_none() {
        if let Some(user) = details
            .get("Config")
            .and_then(|value| value.get("User"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            if let Value::Object(entries) = &mut configuration {
                entries.insert("containerUser".to_string(), Value::String(user.to_string()));
            }
        }
    }

    Ok(InspectedContainerContext {
        remote_workspace_folder: configuration
            .get("workspaceFolder")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| inspect_workspace_mount(details, local_workspace_folder.as_deref())),
        configuration,
        local_workspace_folder,
    })
}

fn inspect_workspace_mount(
    details: &Value,
    local_workspace_folder: Option<&Path>,
) -> Option<String> {
    let mounts = details.get("Mounts").and_then(Value::as_array)?;
    if let Some(local_workspace_folder) = local_workspace_folder {
        let local_workspace_folder = local_workspace_folder.display().to_string();
        if let Some(destination) = mounts.iter().find_map(|mount| {
            (mount.get("Source").and_then(Value::as_str) == Some(local_workspace_folder.as_str()))
                .then(|| mount.get("Destination").and_then(Value::as_str))
                .flatten()
        }) {
            return Some(destination.to_string());
        }
    }
    mounts
        .iter()
        .find_map(|mount| mount.get("Destination").and_then(Value::as_str))
        .map(str::to_string)
}

fn workspace_folder_from_args(args: &[String]) -> Result<Option<PathBuf>, String> {
    if let Some(workspace_folder) = common::parse_option_value(args, "--workspace-folder") {
        return Ok(Some(
            fs::canonicalize(&workspace_folder).unwrap_or_else(|_| PathBuf::from(workspace_folder)),
        ));
    }
    match env::current_dir() {
        Ok(path) => Ok(Some(path)),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        default_remote_workspace_folder, derived_workspace_mount, remote_workspace_folder,
        workspace_mount_for_args, ResolvedConfig,
    };

    #[test]
    fn remote_workspace_folder_prefers_configured_workspace_folder() {
        let resolved = ResolvedConfig {
            workspace_folder: std::path::PathBuf::from("/tmp/example"),
            config_file: std::path::PathBuf::from("/tmp/example/.devcontainer/devcontainer.json"),
            configuration: json!({
                "workspaceFolder": "/configured"
            }),
        };

        assert_eq!(remote_workspace_folder(&resolved), "/configured");
    }

    #[test]
    fn remote_workspace_folder_falls_back_to_workspace_mount_target() {
        let resolved = ResolvedConfig {
            workspace_folder: std::path::PathBuf::from("/tmp/example"),
            config_file: std::path::PathBuf::from("/tmp/example/.devcontainer/devcontainer.json"),
            configuration: json!({
                "workspaceMount": "type=bind,source=/tmp/example,target=/mounted"
            }),
        };

        assert_eq!(remote_workspace_folder(&resolved), "/mounted");
    }

    #[test]
    fn default_remote_workspace_folder_uses_workspace_basename() {
        assert_eq!(
            default_remote_workspace_folder(Some(std::path::Path::new("/tmp/project"))),
            "/workspaces/project"
        );
    }

    #[test]
    fn workspace_mount_for_args_adds_requested_consistency_on_non_linux_hosts() {
        let resolved = ResolvedConfig {
            workspace_folder: std::path::PathBuf::from("/tmp/example"),
            config_file: std::path::PathBuf::from("/tmp/example/.devcontainer/devcontainer.json"),
            configuration: json!({}),
        };
        let mount = workspace_mount_for_args(
            &resolved,
            "/workspaces/example",
            &[
                "--workspace-mount-consistency".to_string(),
                "delegated".to_string(),
            ],
        );
        if std::env::consts::OS == "linux" {
            assert!(!mount.contains("consistency="));
        } else {
            assert!(mount.contains("consistency=delegated"));
        }
    }

    #[test]
    fn derived_workspace_mount_uses_workspace_folder_when_git_root_mount_is_disabled() {
        let workspace = std::env::temp_dir().join("devcontainer-rs-no-git-root");
        let derived = derived_workspace_mount(
            &workspace,
            &[
                "--mount-workspace-git-root".to_string(),
                "false".to_string(),
            ],
        )
        .expect("derived mount");
        assert_eq!(derived.host_mount_folder, workspace);
        assert_eq!(
            derived.remote_workspace_folder,
            "/workspaces/devcontainer-rs-no-git-root"
        );
    }
}
