mod build;
mod compose;
mod container;
mod context;
mod engine;
mod exec;
mod lifecycle;
mod metadata;

use serde_json::{json, Value};

use crate::commands::common;
use crate::process_runner::ProcessResult;

pub enum ExecResult {
    Captured(ProcessResult),
    Streaming(i32),
}

pub fn run_build(args: &[String]) -> Result<Value, String> {
    let resolved = context::load_required_config(args)?;
    let image_name = build::build_image(&resolved, args)?;

    Ok(json!({
        "outcome": "success",
        "command": "build",
        "workspaceFolder": resolved.workspace_folder,
        "configFile": resolved.config_file,
        "imageName": image_name,
        "configuration": resolved.configuration,
    }))
}

pub fn run_up(args: &[String]) -> Result<Value, String> {
    let resolved = context::load_required_config(args)?;
    lifecycle::run_initialize_command(&resolved.configuration, &resolved.workspace_folder)?;
    let image_name = build::runtime_image_name(&resolved, args)?;
    let remote_workspace_folder = context::remote_workspace_folder(&resolved);
    let up_container =
        container::ensure_up_container(&resolved, args, &image_name, &remote_workspace_folder)?;
    lifecycle::run_lifecycle_commands(
        &up_container.container_id,
        args,
        &resolved.configuration,
        &remote_workspace_folder,
        up_container.lifecycle_mode,
    )?;

    Ok(json!({
        "outcome": "success",
        "command": "up",
        "containerId": up_container.container_id,
        "remoteUser": context::remote_user(&resolved.configuration),
        "remoteWorkspaceFolder": remote_workspace_folder,
        "configuration": if common::has_flag(args, "--include-configuration") { resolved.configuration.clone() } else { Value::Null },
        "mergedConfiguration": if common::has_flag(args, "--include-merged-configuration") { resolved.configuration.clone() } else { Value::Null },
        "workspaceFolder": resolved.workspace_folder,
        "configFile": resolved.config_file,
    }))
}

pub fn run_set_up(args: &[String]) -> Result<Value, String> {
    let context = context::resolve_existing_container_context(args)?;
    lifecycle::run_lifecycle_commands(
        &context.container_id,
        args,
        &context.configuration,
        &context.remote_workspace_folder,
        lifecycle::LifecycleMode::SetUp,
    )?;

    Ok(json!({
        "outcome": "success",
        "command": "set-up",
        "containerId": context.container_id,
        "configuration": if common::has_flag(args, "--include-configuration") { context.configuration.clone() } else { Value::Null },
        "mergedConfiguration": if common::has_flag(args, "--include-merged-configuration") { context.configuration } else { Value::Null },
        "remoteWorkspaceFolder": context.remote_workspace_folder,
    }))
}

pub fn run_user_commands(args: &[String]) -> Result<Value, String> {
    let context = context::resolve_existing_container_context(args)?;
    lifecycle::run_lifecycle_commands(
        &context.container_id,
        args,
        &context.configuration,
        &context.remote_workspace_folder,
        lifecycle::LifecycleMode::RunUserCommands,
    )?;

    Ok(json!({
        "outcome": "success",
        "command": "run-user-commands",
        "containerId": context.container_id,
        "remoteWorkspaceFolder": context.remote_workspace_folder,
    }))
}

pub fn run_exec(args: &[String]) -> Result<ExecResult, String> {
    let command_args = exec::exec_command_and_args(args)?;
    let context = context::resolve_existing_container_context(args)?;
    let interactive = common::has_flag(args, "--interactive");
    let engine_args = exec::exec_engine_args(
        args,
        &context.configuration,
        &context.remote_workspace_folder,
        &context.container_id,
        command_args,
        interactive,
    );

    if interactive {
        engine::run_engine_streaming(args, engine_args).map(ExecResult::Streaming)
    } else {
        engine::run_engine(args, engine_args).map(ExecResult::Captured)
    }
}
