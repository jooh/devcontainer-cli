use std::path::Path;
use std::thread;

use serde_json::Value;

use crate::commands::common;
use crate::process_runner::{self, ProcessRequest};

use super::context::{combined_remote_env, configured_user};
use super::engine;

#[derive(Clone, Copy)]
pub(crate) enum LifecycleMode {
    UpCreated,
    UpStarted,
    UpReused,
    SetUp,
    RunUserCommands,
}

enum LifecycleCommand {
    Shell(String),
    Exec(Vec<String>),
}

pub(crate) fn run_lifecycle_commands(
    container_id: &str,
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    mode: LifecycleMode,
) -> Result<(), String> {
    for command_group in selected_lifecycle_commands(configuration, args, mode) {
        run_process_group(command_group, |command| {
            engine::engine_request(
                args,
                lifecycle_exec_args(
                    args,
                    configuration,
                    remote_workspace_folder,
                    container_id,
                    command,
                ),
            )
        })?;
    }

    Ok(())
}

pub(crate) fn run_initialize_command(
    configuration: &Value,
    workspace_folder: &Path,
) -> Result<(), String> {
    let Some(command_group) = lifecycle_command_value(configuration, "initializeCommand") else {
        return Ok(());
    };

    run_process_group(command_group, |command| {
        host_lifecycle_request(workspace_folder, command)
    })
}

fn run_process_group(
    command_group: Vec<LifecycleCommand>,
    build_request: impl Fn(LifecycleCommand) -> ProcessRequest,
) -> Result<(), String> {
    if command_group.len() == 1 {
        let result = process_runner::run_process(&build_request(
            command_group
                .into_iter()
                .next()
                .expect("single lifecycle command"),
        ))?;
        if result.status_code != 0 {
            return Err(engine::stderr_or_stdout(&result));
        }
        return Ok(());
    }

    let handles = command_group
        .into_iter()
        .map(|command| {
            let request = build_request(command);
            thread::spawn(move || process_runner::run_process(&request))
        })
        .collect::<Vec<_>>();

    let mut first_error = None;
    for handle in handles {
        match handle.join() {
            Ok(Ok(result)) if result.status_code == 0 => {}
            Ok(Ok(result)) => {
                if first_error.is_none() {
                    first_error = Some(engine::stderr_or_stdout(&result));
                }
            }
            Ok(Err(error)) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            Err(_) => {
                if first_error.is_none() {
                    first_error =
                        Some("Lifecycle command thread panicked unexpectedly".to_string());
                }
            }
        }
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(())
}

fn selected_lifecycle_commands(
    configuration: &Value,
    args: &[String],
    mode: LifecycleMode,
) -> Vec<Vec<LifecycleCommand>> {
    let skip_post_create = common::has_flag(args, "--skip-post-create");
    let skip_post_attach = common::has_flag(args, "--skip-post-attach");
    let skip_non_blocking = common::has_flag(args, "--skip-non-blocking-commands");

    if skip_post_create {
        return Vec::new();
    }

    let wait_for = configuration
        .get("waitFor")
        .and_then(Value::as_str)
        .unwrap_or("updateContentCommand");
    if skip_non_blocking && wait_for == "initializeCommand" {
        return Vec::new();
    }

    let mut commands = Vec::new();
    let lifecycle_stages = [
        (
            "onCreateCommand",
            lifecycle_command_value(configuration, "onCreateCommand"),
        ),
        (
            "updateContentCommand",
            lifecycle_command_value(configuration, "updateContentCommand"),
        ),
        (
            "postCreateCommand",
            lifecycle_command_value(configuration, "postCreateCommand"),
        ),
        (
            "postStartCommand",
            lifecycle_command_value(configuration, "postStartCommand"),
        ),
        (
            "postAttachCommand",
            (!skip_post_attach)
                .then(|| lifecycle_command_value(configuration, "postAttachCommand"))
                .flatten(),
        ),
    ];

    for (stage, command_group) in lifecycle_stages {
        if lifecycle_stage_runs_in_mode(stage, mode) {
            if let Some(command_group) = command_group {
                commands.push(command_group);
            }
        }
        if skip_non_blocking && stage == wait_for {
            break;
        }
    }

    commands
}

fn lifecycle_stage_runs_in_mode(stage: &str, mode: LifecycleMode) -> bool {
    match mode {
        LifecycleMode::UpCreated | LifecycleMode::SetUp | LifecycleMode::RunUserCommands => true,
        LifecycleMode::UpStarted => matches!(stage, "postStartCommand" | "postAttachCommand"),
        LifecycleMode::UpReused => stage == "postAttachCommand",
    }
}

fn lifecycle_command_value(configuration: &Value, key: &str) -> Option<Vec<LifecycleCommand>> {
    let value = configuration.get(key)?;
    lifecycle_command_group(value)
}

fn lifecycle_command_group(value: &Value) -> Option<Vec<LifecycleCommand>> {
    match value {
        Value::String(command) => Some(vec![LifecycleCommand::Shell(command.clone())]),
        Value::Array(parts) => {
            command_parts(parts).map(|parts| vec![LifecycleCommand::Exec(parts)])
        }
        Value::Object(entries) => {
            let commands = entries
                .values()
                .filter_map(lifecycle_command)
                .collect::<Vec<_>>();
            if commands.is_empty() {
                None
            } else {
                Some(commands)
            }
        }
        _ => None,
    }
}

fn lifecycle_command(value: &Value) -> Option<LifecycleCommand> {
    match value {
        Value::String(command) => Some(LifecycleCommand::Shell(command.clone())),
        Value::Array(parts) => command_parts(parts).map(LifecycleCommand::Exec),
        _ => None,
    }
}

fn command_parts(parts: &[Value]) -> Option<Vec<String>> {
    let parts = parts
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn lifecycle_exec_args(
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    container_id: &str,
    command: LifecycleCommand,
) -> Vec<String> {
    let mut engine_args = vec![
        "exec".to_string(),
        "--workdir".to_string(),
        remote_workspace_folder.to_string(),
    ];
    if let Some(user) = configured_user(configuration) {
        engine_args.push("--user".to_string());
        engine_args.push(user.to_string());
    }
    for (key, value) in combined_remote_env(args, Some(configuration)) {
        engine_args.push("-e".to_string());
        engine_args.push(format!("{key}={value}"));
    }
    engine_args.push(container_id.to_string());
    match command {
        LifecycleCommand::Shell(command) => {
            engine_args.push("sh".to_string());
            engine_args.push("-lc".to_string());
            engine_args.push(command);
        }
        LifecycleCommand::Exec(parts) => engine_args.extend(parts),
    }
    engine_args
}

fn host_lifecycle_request(workspace_folder: &Path, command: LifecycleCommand) -> ProcessRequest {
    match command {
        LifecycleCommand::Shell(command) => ProcessRequest {
            program: "/bin/sh".to_string(),
            args: vec!["-lc".to_string(), command],
            cwd: Some(workspace_folder.to_path_buf()),
            env: std::collections::HashMap::new(),
        },
        LifecycleCommand::Exec(parts) => {
            let mut parts = parts.into_iter();
            ProcessRequest {
                program: parts.next().unwrap_or_default(),
                args: parts.collect(),
                cwd: Some(workspace_folder.to_path_buf()),
                env: std::collections::HashMap::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{lifecycle_command_group, selected_lifecycle_commands, LifecycleMode};

    #[test]
    fn lifecycle_command_group_supports_strings_arrays_and_objects() {
        assert!(lifecycle_command_group(&json!("echo hello")).is_some());
        assert!(lifecycle_command_group(&json!(["/bin/echo", "hello"])).is_some());
        assert!(lifecycle_command_group(&json!({
            "a": "echo one",
            "b": ["/bin/echo", "two"]
        }))
        .is_some());
    }

    #[test]
    fn selected_lifecycle_commands_respect_mode_and_wait_for() {
        let commands = selected_lifecycle_commands(
            &json!({
                "onCreateCommand": "echo on-create",
                "updateContentCommand": "echo update",
                "postCreateCommand": "echo post-create",
                "postStartCommand": "echo post-start",
                "postAttachCommand": "echo post-attach",
                "waitFor": "postStartCommand"
            }),
            &["--skip-non-blocking-commands".to_string()],
            LifecycleMode::RunUserCommands,
        );

        assert_eq!(commands.len(), 4);

        let reused = selected_lifecycle_commands(
            &json!({
                "postStartCommand": "echo post-start",
                "postAttachCommand": "echo post-attach"
            }),
            &[],
            LifecycleMode::UpReused,
        );

        assert_eq!(reused.len(), 1);
    }
}
