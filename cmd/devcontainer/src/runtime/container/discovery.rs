//! Container discovery, reuse, and creation orchestration for native runtime flows.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::commands::common;

use super::super::compose;
use super::super::context::ResolvedConfig;
use super::super::engine;
use super::super::lifecycle::LifecycleMode;
use super::engine_run::{remove_container, start_container, start_existing_container};
use super::UpContainer;

pub(crate) fn ensure_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    if compose::uses_compose_config(&resolved.configuration) {
        return ensure_compose_up_container(resolved, args, image_name, remote_workspace_folder);
    }

    ensure_engine_up_container(resolved, args, image_name, remote_workspace_folder)
}

fn ensure_compose_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    let remove_existing = common::has_flag(args, "--remove-existing-container");
    if let Some(container_id) = compose::resolve_container_id(resolved, args)? {
        if remove_existing {
            compose::remove_service(resolved, args)?;
            return create_compose_container(resolved, args, image_name, remote_workspace_folder);
        }
        return refresh_compose_container(
            resolved,
            args,
            image_name,
            remote_workspace_folder,
            &container_id,
            LifecycleMode::UpReused,
        );
    }

    if let Some(container_id) = compose::resolve_container_id_including_stopped(resolved, args)? {
        if remove_existing {
            compose::remove_service(resolved, args)?;
            return create_compose_container(resolved, args, image_name, remote_workspace_folder);
        }
        return refresh_compose_container(
            resolved,
            args,
            image_name,
            remote_workspace_folder,
            &container_id,
            LifecycleMode::UpStarted,
        );
    }

    if common::has_flag(args, "--expect-existing-container") {
        return Err("Dev container not found.".to_string());
    }

    create_compose_container(resolved, args, image_name, remote_workspace_folder)
}

fn ensure_engine_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    let running = find_target_container(
        args,
        Some(resolved.workspace_folder.as_path()),
        Some(resolved.config_file.as_path()),
        false,
    )?;
    let remove_existing = common::has_flag(args, "--remove-existing-container");
    match running {
        Some(container_id) if remove_existing => {
            remove_container(args, &container_id)?;
            create_engine_container(resolved, args, image_name, remote_workspace_folder)
        }
        Some(container_id) => Ok(UpContainer {
            container_id,
            lifecycle_mode: LifecycleMode::UpReused,
        }),
        None => match find_target_container(
            args,
            Some(resolved.workspace_folder.as_path()),
            Some(resolved.config_file.as_path()),
            true,
        )? {
            Some(container_id) if remove_existing => {
                remove_container(args, &container_id)?;
                create_engine_container(resolved, args, image_name, remote_workspace_folder)
            }
            Some(container_id) => {
                start_existing_container(args, &container_id)?;
                Ok(UpContainer {
                    container_id,
                    lifecycle_mode: LifecycleMode::UpStarted,
                })
            }
            None if common::has_flag(args, "--expect-existing-container") => {
                Err("Dev container not found.".to_string())
            }
            None => create_engine_container(resolved, args, image_name, remote_workspace_folder),
        },
    }
}

fn create_compose_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    compose::up_service(resolved, args, remote_workspace_folder, image_name, false)?;
    let container_id = compose::resolve_container_id(resolved, args)?
        .ok_or_else(|| "Dev container not found.".to_string())?;
    Ok(UpContainer {
        container_id,
        lifecycle_mode: LifecycleMode::UpCreated,
    })
}

fn refresh_compose_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
    previous_container_id: &str,
    unchanged_mode: LifecycleMode,
) -> Result<UpContainer, String> {
    compose::up_service(resolved, args, remote_workspace_folder, image_name, true)?;
    let updated_container_id = compose::resolve_container_id(resolved, args)?
        .ok_or_else(|| "Dev container not found.".to_string())?;
    Ok(UpContainer {
        lifecycle_mode: if updated_container_id == previous_container_id {
            unchanged_mode
        } else {
            LifecycleMode::UpCreated
        },
        container_id: updated_container_id,
    })
}

fn create_engine_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    start_container(resolved, args, image_name, remote_workspace_folder).map(|container_id| {
        UpContainer {
            container_id,
            lifecycle_mode: LifecycleMode::UpCreated,
        }
    })
}

pub(crate) fn resolve_target_container(
    args: &[String],
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
) -> Result<String, String> {
    if let Some(container_id) = common::parse_option_value(args, "--container-id") {
        return Ok(container_id);
    }

    match find_target_container(args, workspace_folder, config_file, false)? {
        Some(container_id) => Ok(container_id),
        None => Err("Dev container not found.".to_string()),
    }
}

fn find_target_container(
    args: &[String],
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
    include_stopped: bool,
) -> Result<Option<String>, String> {
    let labels = target_container_labels(args, workspace_folder, config_file);
    if labels.is_empty() {
        return Err(
            "Unable to determine target container. Provide --container-id or --workspace-folder."
                .to_string(),
        );
    }

    if let Some(container_id) = query_target_container(args, &labels, include_stopped)? {
        return Ok(Some(container_id));
    }

    if !common::parse_option_values(args, "--id-label").is_empty()
        || std::env::consts::OS != "windows"
    {
        return Ok(None);
    }

    find_normalized_default_label_match(args, workspace_folder, config_file, include_stopped)
}

fn query_target_container(
    args: &[String],
    labels: &[String],
    include_stopped: bool,
) -> Result<Option<String>, String> {
    let result = engine::run_engine(args, ps_engine_args(labels, include_stopped))?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    Ok(parse_container_ids(&result.stdout).into_iter().next())
}

fn ps_engine_args(labels: &[String], include_stopped: bool) -> Vec<String> {
    let mut engine_args = vec!["ps".to_string(), "-q".to_string()];
    if include_stopped {
        engine_args.push("-a".to_string());
    }
    for label in labels {
        engine_args.push("--filter".to_string());
        engine_args.push(format!("label={label}"));
    }
    engine_args
}

fn find_normalized_default_label_match(
    args: &[String],
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
    include_stopped: bool,
) -> Result<Option<String>, String> {
    let Some(workspace_folder) = workspace_folder else {
        return Ok(None);
    };
    let [(_, normalized_workspace), (_, normalized_config)] =
        common::default_devcontainer_id_label_pairs(
            workspace_folder,
            config_file.unwrap_or(workspace_folder),
        );
    let candidate_ids = list_container_ids_by_label_name(
        args,
        common::DEVCONTAINER_LOCAL_FOLDER_LABEL,
        include_stopped,
    )?;
    let mut legacy_match = None;
    for container_id in candidate_ids {
        let Some(labels) = inspect_container_labels(args, &container_id)? else {
            continue;
        };
        match normalized_default_label_match(
            &labels,
            normalized_workspace.as_str(),
            config_file.map(|_| normalized_config.as_str()),
            common::DEVCONTAINER_LOCAL_FOLDER_LABEL,
            common::DEVCONTAINER_CONFIG_FILE_LABEL,
        ) {
            Some(DefaultLabelMatch::Current) => return Ok(Some(container_id)),
            Some(DefaultLabelMatch::Legacy) if legacy_match.is_none() => {
                legacy_match = Some(container_id);
            }
            _ => {}
        }
    }
    Ok(legacy_match)
}

fn list_container_ids_by_label_name(
    args: &[String],
    label_name: &str,
    include_stopped: bool,
) -> Result<Vec<String>, String> {
    let result = engine::run_engine(
        args,
        ps_engine_args(&[label_name.to_string()], include_stopped),
    )?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(parse_container_ids(&result.stdout))
}

fn parse_container_ids(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.chars().any(char::is_whitespace))
        .map(str::to_string)
        .collect()
}

fn inspect_container_labels(
    args: &[String],
    container_id: &str,
) -> Result<Option<HashMap<String, String>>, String> {
    let result = engine::run_engine(args, vec!["inspect".to_string(), container_id.to_string()])?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    let inspected: Value = serde_json::from_str(&result.stdout)
        .map_err(|error| format!("Invalid inspect JSON: {error}"))?;
    Ok(inspected
        .as_array()
        .and_then(|entries| entries.first())
        .and_then(|details| details.get("Config"))
        .and_then(|config| config.get("Labels"))
        .and_then(Value::as_object)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultLabelMatch {
    Current,
    Legacy,
}

fn normalized_default_label_match(
    labels: &HashMap<String, String>,
    normalized_workspace: &str,
    normalized_config: Option<&str>,
    workspace_key: &str,
    config_key: &str,
) -> Option<DefaultLabelMatch> {
    let workspace_value = labels
        .get(workspace_key)
        .map(|value| common::normalize_devcontainer_label_path_for_platform("windows", value))?;
    if workspace_value != normalized_workspace {
        return None;
    }

    match (
        normalized_config,
        labels
            .get(config_key)
            .map(|value| common::normalize_devcontainer_label_path_for_platform("windows", value)),
    ) {
        (Some(target_config), Some(container_config)) if container_config == target_config => {
            Some(DefaultLabelMatch::Current)
        }
        (Some(_), None) => Some(DefaultLabelMatch::Legacy),
        (None, _) => Some(DefaultLabelMatch::Current),
        _ => None,
    }
}

fn target_container_labels(
    args: &[String],
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
) -> Vec<String> {
    let mut labels = common::parse_option_values(args, "--id-label");
    if labels.is_empty() {
        if let (Some(workspace_folder), Some(config_file)) = (workspace_folder, config_file) {
            labels.extend(common::default_devcontainer_id_labels(
                workspace_folder,
                config_file,
            ));
        } else if let Some(workspace_folder) = workspace_folder {
            let [(workspace_key, workspace_value), _] =
                common::default_devcontainer_id_label_pairs(workspace_folder, workspace_folder);
            labels.push(format!("{workspace_key}={workspace_value}"));
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{normalized_default_label_match, DefaultLabelMatch};
    use crate::commands::common;

    #[test]
    fn normalized_default_label_match_accepts_windows_path_casing_changes() {
        let mut labels = HashMap::new();
        labels.insert(
            common::DEVCONTAINER_LOCAL_FOLDER_LABEL.to_string(),
            "C:\\CodeBlocks\\remill".to_string(),
        );
        labels.insert(
            common::DEVCONTAINER_CONFIG_FILE_LABEL.to_string(),
            "C:/CodeBlocks/remill/.devcontainer/devcontainer.json".to_string(),
        );

        let label_match = normalized_default_label_match(
            &labels,
            "c:\\CodeBlocks\\remill",
            Some("c:\\CodeBlocks\\remill\\.devcontainer\\devcontainer.json"),
            common::DEVCONTAINER_LOCAL_FOLDER_LABEL,
            common::DEVCONTAINER_CONFIG_FILE_LABEL,
        );

        assert_eq!(label_match, Some(DefaultLabelMatch::Current));
    }

    #[test]
    fn normalized_default_label_match_keeps_legacy_workspace_only_matches() {
        let mut labels = HashMap::new();
        labels.insert(
            common::DEVCONTAINER_LOCAL_FOLDER_LABEL.to_string(),
            "C:\\CodeBlocks\\remill".to_string(),
        );

        let label_match = normalized_default_label_match(
            &labels,
            "c:\\CodeBlocks\\remill",
            Some("c:\\CodeBlocks\\remill\\.devcontainer\\devcontainer.json"),
            common::DEVCONTAINER_LOCAL_FOLDER_LABEL,
            common::DEVCONTAINER_CONFIG_FILE_LABEL,
        );

        assert_eq!(label_match, Some(DefaultLabelMatch::Legacy));
    }
}
