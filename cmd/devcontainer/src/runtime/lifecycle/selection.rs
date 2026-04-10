//! Lifecycle stage selection and lifecycle-command parsing helpers.

use serde_json::Value;

use crate::commands::common;

use super::{dotfiles, LifecycleCommand, LifecycleMode, LifecycleStep};

pub(super) fn selected_lifecycle_steps(
    configuration: &Value,
    args: &[String],
    mode: LifecycleMode,
) -> Vec<LifecycleStep> {
    let skip_post_create = common::has_flag(args, "--skip-post-create");
    let skip_post_attach = common::has_flag(args, "--skip-post-attach");
    let skip_non_blocking = common::has_flag(args, "--skip-non-blocking-commands");
    let stop_for_personalization = common::runtime_options(args).stop_for_personalization;

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

    let mut steps = Vec::new();
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
                steps.push(LifecycleStep::CommandGroup(command_group));
            }
        }
        if skip_non_blocking && stage == wait_for {
            break;
        }
        if stage == "postCreateCommand" && lifecycle_stage_runs_in_mode(stage, mode) {
            if dotfiles::dotfiles_install_command(args).is_some() {
                steps.push(LifecycleStep::InstallDotfiles);
            }
            if stop_for_personalization {
                break;
            }
        }
    }

    steps
}

fn lifecycle_stage_runs_in_mode(stage: &str, mode: LifecycleMode) -> bool {
    match mode {
        LifecycleMode::UpCreated | LifecycleMode::SetUp | LifecycleMode::RunUserCommands => true,
        LifecycleMode::UpStarted => matches!(stage, "postStartCommand" | "postAttachCommand"),
        LifecycleMode::UpReused => stage == "postAttachCommand",
    }
}

pub(super) fn lifecycle_command_value(
    configuration: &Value,
    key: &str,
) -> Option<Vec<LifecycleCommand>> {
    let value = configuration.get(key)?;
    lifecycle_command_group(value)
}

pub(super) fn lifecycle_command_group(value: &Value) -> Option<Vec<LifecycleCommand>> {
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
