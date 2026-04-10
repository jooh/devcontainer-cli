//! Lifecycle process request construction for container and host commands.

use std::path::Path;

use serde_json::Value;

use crate::commands::common;
use crate::process_runner::ProcessRequest;

use super::super::context::{combined_remote_env, configured_user};
use super::LifecycleCommand;

pub(super) fn lifecycle_exec_args(
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    container_id: &str,
    command: LifecycleCommand,
) -> Result<Vec<String>, String> {
    let mut engine_args = vec![
        "exec".to_string(),
        "--workdir".to_string(),
        remote_workspace_folder.to_string(),
    ];
    if let Some(user) = configured_user(configuration) {
        engine_args.push("--user".to_string());
        engine_args.push(user.to_string());
    }
    for (key, value) in combined_remote_env(args, Some(configuration))? {
        engine_args.push("-e".to_string());
        engine_args.push(format!("{key}={value}"));
    }
    engine_args.push(container_id.to_string());
    match command {
        LifecycleCommand::Shell(command) => {
            engine_args.push("/bin/sh".to_string());
            engine_args.push("-lc".to_string());
            engine_args.push(command);
        }
        LifecycleCommand::Exec(parts) => engine_args.extend(parts),
    }
    Ok(engine_args)
}

pub(super) fn host_lifecycle_request(
    args: &[String],
    workspace_folder: &Path,
    command: LifecycleCommand,
) -> ProcessRequest {
    match command {
        LifecycleCommand::Shell(command) => common::runtime_process_request(
            args,
            "/bin/sh".to_string(),
            vec!["-c".to_string(), command],
            Some(workspace_folder.to_path_buf()),
        ),
        LifecycleCommand::Exec(parts) => {
            let mut parts = parts.into_iter();
            common::runtime_process_request(
                args,
                parts.next().unwrap_or_default(),
                parts.collect(),
                Some(workspace_folder.to_path_buf()),
            )
        }
    }
}
