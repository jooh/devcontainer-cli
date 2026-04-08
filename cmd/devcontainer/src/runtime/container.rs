use serde_json::Value;

use crate::commands::common;

use super::compose;
use super::context::{workspace_mount, ResolvedConfig};
use super::engine;
use super::lifecycle::LifecycleMode;
use super::metadata::serialized_container_metadata;

pub(crate) struct UpContainer {
    pub(crate) container_id: String,
    pub(crate) lifecycle_mode: LifecycleMode,
}

pub(crate) fn ensure_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    if compose::uses_compose_config(&resolved.configuration) {
        let running = compose::resolve_container_id(resolved, args)?;
        match running {
            Some(_) if common::has_flag(args, "--remove-existing-container") => {
                compose::remove_service(resolved, args)?;
                compose::up_service(resolved, args, remote_workspace_folder)?;
                let container_id = compose::resolve_container_id(resolved, args)?
                    .ok_or_else(|| "Dev container not found.".to_string())?;
                return Ok(UpContainer {
                    container_id,
                    lifecycle_mode: LifecycleMode::UpCreated,
                });
            }
            Some(container_id) => {
                compose::up_service(resolved, args, remote_workspace_folder)?;
                let updated_container_id = compose::resolve_container_id(resolved, args)?
                    .ok_or_else(|| "Dev container not found.".to_string())?;
                return Ok(UpContainer {
                    lifecycle_mode: if updated_container_id == container_id {
                        LifecycleMode::UpReused
                    } else {
                        LifecycleMode::UpCreated
                    },
                    container_id: updated_container_id,
                });
            }
            None => {
                let existing = compose::resolve_container_id_including_stopped(resolved, args)?;
                match existing {
                    Some(_) if common::has_flag(args, "--remove-existing-container") => {
                        compose::remove_service(resolved, args)?;
                        compose::up_service(resolved, args, remote_workspace_folder)?;
                        let container_id = compose::resolve_container_id(resolved, args)?
                            .ok_or_else(|| "Dev container not found.".to_string())?;
                        return Ok(UpContainer {
                            container_id,
                            lifecycle_mode: LifecycleMode::UpCreated,
                        });
                    }
                    Some(container_id) => {
                        compose::up_service(resolved, args, remote_workspace_folder)?;
                        let updated_container_id =
                            compose::resolve_container_id(resolved, args)?
                                .ok_or_else(|| "Dev container not found.".to_string())?;
                        return Ok(UpContainer {
                            lifecycle_mode: if updated_container_id == container_id {
                                LifecycleMode::UpStarted
                            } else {
                                LifecycleMode::UpCreated
                            },
                            container_id: updated_container_id,
                        });
                    }
                    None if common::has_flag(args, "--expect-existing-container") => {
                        return Err("Dev container not found.".to_string());
                    }
                    None => {
                        compose::up_service(resolved, args, remote_workspace_folder)?;
                        let container_id = compose::resolve_container_id(resolved, args)?
                            .ok_or_else(|| "Dev container not found.".to_string())?;
                        return Ok(UpContainer {
                            container_id,
                            lifecycle_mode: LifecycleMode::UpCreated,
                        });
                    }
                }
            }
        }
    }

    let running = find_target_container(
        args,
        Some(resolved.workspace_folder.as_path()),
        Some(resolved.config_file.as_path()),
        false,
    )?;
    match running {
        Some(container_id) if common::has_flag(args, "--remove-existing-container") => {
            remove_container(args, &container_id)?;
            start_container(resolved, args, image_name, remote_workspace_folder).map(
                |container_id| UpContainer {
                    container_id,
                    lifecycle_mode: LifecycleMode::UpCreated,
                },
            )
        }
        Some(container_id) => Ok(UpContainer {
            container_id,
            lifecycle_mode: LifecycleMode::UpReused,
        }),
        None => {
            let existing = find_target_container(
                args,
                Some(resolved.workspace_folder.as_path()),
                Some(resolved.config_file.as_path()),
                true,
            )?;
            match existing {
                Some(container_id) if common::has_flag(args, "--remove-existing-container") => {
                    remove_container(args, &container_id)?;
                    start_container(resolved, args, image_name, remote_workspace_folder).map(
                        |container_id| UpContainer {
                            container_id,
                            lifecycle_mode: LifecycleMode::UpCreated,
                        },
                    )
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
                None => start_container(resolved, args, image_name, remote_workspace_folder).map(
                    |container_id| UpContainer {
                        container_id,
                        lifecycle_mode: LifecycleMode::UpCreated,
                    },
                ),
            }
        }
    }
}

pub(crate) fn resolve_target_container(
    args: &[String],
    workspace_folder: Option<&std::path::Path>,
    config_file: Option<&std::path::Path>,
) -> Result<String, String> {
    if let Some(container_id) = common::parse_option_value(args, "--container-id") {
        return Ok(container_id);
    }

    match find_target_container(args, workspace_folder, config_file, false)? {
        Some(container_id) => Ok(container_id),
        None => Err("Dev container not found.".to_string()),
    }
}

fn start_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<String, String> {
    let mut engine_args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--label".to_string(),
        format!(
            "devcontainer.local_folder={}",
            resolved.workspace_folder.display()
        ),
        "--label".to_string(),
        format!(
            "devcontainer.config_file={}",
            resolved.config_file.display()
        ),
        "--label".to_string(),
        format!(
            "devcontainer.metadata={}",
            serialized_container_metadata(&resolved.configuration, remote_workspace_folder)?
        ),
        "--mount".to_string(),
        workspace_mount(resolved, remote_workspace_folder),
    ];
    for label in common::parse_option_values(args, "--id-label") {
        engine_args.push("--label".to_string());
        engine_args.push(label);
    }
    for mount in common::parse_option_values(args, "--mount") {
        engine_args.push("--mount".to_string());
        engine_args.push(mount);
    }
    if let Some(run_args) = resolved
        .configuration
        .get("runArgs")
        .and_then(Value::as_array)
    {
        for arg in run_args.iter().filter_map(Value::as_str) {
            engine_args.push(arg.to_string());
        }
    }
    if let Some(container_env) = resolved
        .configuration
        .get("containerEnv")
        .and_then(Value::as_object)
    {
        for (key, value) in container_env {
            if let Some(value) = value.as_str() {
                engine_args.push("-e".to_string());
                engine_args.push(format!("{key}={value}"));
            }
        }
    }
    engine_args.push(image_name.to_string());
    engine_args.push("/bin/sh".to_string());
    engine_args.push("-lc".to_string());
    engine_args.push("while sleep 1000; do :; done".to_string());

    let result = engine::run_engine(args, engine_args)?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    let container_id = result.stdout.trim().to_string();
    if container_id.is_empty() {
        return Err("Container engine did not return a container id".to_string());
    }

    Ok(container_id)
}

fn start_existing_container(args: &[String], container_id: &str) -> Result<(), String> {
    let result = engine::run_engine(args, vec!["start".to_string(), container_id.to_string()])?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

fn remove_container(args: &[String], container_id: &str) -> Result<(), String> {
    let result = engine::run_engine(
        args,
        vec!["rm".to_string(), "-f".to_string(), container_id.to_string()],
    )?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

fn find_target_container(
    args: &[String],
    workspace_folder: Option<&std::path::Path>,
    config_file: Option<&std::path::Path>,
    include_stopped: bool,
) -> Result<Option<String>, String> {
    let labels = target_container_labels(args, workspace_folder, config_file);
    if labels.is_empty() {
        return Err(
            "Unable to determine target container. Provide --container-id or --workspace-folder."
                .to_string(),
        );
    }

    let mut engine_args = vec!["ps".to_string(), "-q".to_string()];
    if include_stopped {
        engine_args.push("-a".to_string());
    }
    for label in labels {
        engine_args.push("--filter".to_string());
        engine_args.push(format!("label={label}"));
    }

    let result = engine::run_engine(args, engine_args)?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    Ok(result
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.chars().any(char::is_whitespace))
        .map(str::to_string))
}

fn target_container_labels(
    args: &[String],
    workspace_folder: Option<&std::path::Path>,
    config_file: Option<&std::path::Path>,
) -> Vec<String> {
    let mut labels = common::parse_option_values(args, "--id-label");
    if labels.is_empty() {
        if let Some(workspace_folder) = workspace_folder {
            labels.push(format!(
                "devcontainer.local_folder={}",
                workspace_folder.display()
            ));
        }
        if let Some(config_file) = config_file {
            labels.push(format!(
                "devcontainer.config_file={}",
                config_file.display()
            ));
        }
    }
    labels
}
