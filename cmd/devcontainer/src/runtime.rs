use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::thread;

use serde_json::{json, Map, Value};

use crate::commands::common;
use crate::config::{self, ConfigContext};
use crate::process_runner::{self, ProcessRequest, ProcessResult};

pub enum ExecResult {
    Captured(ProcessResult),
    Streaming(i32),
}

pub fn run_build(args: &[String]) -> Result<Value, String> {
    let resolved = load_required_config(args)?;
    let image_name = build_image(&resolved, args)?;

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
    let resolved = load_required_config(args)?;
    run_initialize_command(&resolved.configuration, &resolved.workspace_folder)?;
    let image_name = runtime_image_name(&resolved, args)?;
    let remote_workspace_folder = remote_workspace_folder(&resolved);
    let container_id = ensure_up_container(&resolved, args, &image_name, &remote_workspace_folder)?;
    run_lifecycle_commands(
        &container_id,
        args,
        &resolved.configuration,
        &remote_workspace_folder,
        LifecycleMode::Up,
    )?;

    Ok(json!({
        "outcome": "success",
        "command": "up",
        "containerId": container_id,
        "remoteUser": remote_user(&resolved.configuration),
        "remoteWorkspaceFolder": remote_workspace_folder,
        "configuration": if common::has_flag(args, "--include-configuration") { resolved.configuration.clone() } else { Value::Null },
        "mergedConfiguration": if common::has_flag(args, "--include-merged-configuration") { resolved.configuration.clone() } else { Value::Null },
        "workspaceFolder": resolved.workspace_folder,
        "configFile": resolved.config_file,
    }))
}

pub fn run_set_up(args: &[String]) -> Result<Value, String> {
    let resolved = load_optional_config(args)?;
    let workspace_folder = resolved
        .as_ref()
        .map(|value| value.workspace_folder.as_path());
    let container_id = resolve_target_container(
        args,
        workspace_folder,
        resolved.as_ref().map(|value| value.config_file.as_path()),
    )?;
    let fallback_workspace = workspace_folder_from_args(args)?;
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
        .map(remote_workspace_folder)
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
                    .or(fallback_workspace.as_deref()),
            )
        });

    run_lifecycle_commands(
        &container_id,
        args,
        &configuration,
        &remote_workspace_folder,
        LifecycleMode::SetUp,
    )?;

    Ok(json!({
        "outcome": "success",
        "command": "set-up",
        "containerId": container_id,
        "configuration": if common::has_flag(args, "--include-configuration") { configuration.clone() } else { Value::Null },
        "mergedConfiguration": if common::has_flag(args, "--include-merged-configuration") { configuration } else { Value::Null },
        "remoteWorkspaceFolder": remote_workspace_folder,
    }))
}

pub fn run_user_commands(args: &[String]) -> Result<Value, String> {
    let resolved = load_optional_config(args)?;
    let workspace_folder = resolved
        .as_ref()
        .map(|value| value.workspace_folder.as_path());
    let container_id = resolve_target_container(
        args,
        workspace_folder,
        resolved.as_ref().map(|value| value.config_file.as_path()),
    )?;
    let fallback_workspace = workspace_folder_from_args(args)?;
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
        .map(remote_workspace_folder)
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
                    .or(fallback_workspace.as_deref()),
            )
        });

    run_lifecycle_commands(
        &container_id,
        args,
        &configuration,
        &remote_workspace_folder,
        LifecycleMode::RunUserCommands,
    )?;

    Ok(json!({
        "outcome": "success",
        "command": "run-user-commands",
        "containerId": container_id,
        "remoteWorkspaceFolder": remote_workspace_folder,
    }))
}

pub fn run_exec(args: &[String]) -> Result<ExecResult, String> {
    let command_args = exec_command_and_args(args)?;
    let resolved = load_optional_config(args)?;
    let workspace_folder = if let Some(resolved) = &resolved {
        Some(resolved.workspace_folder.clone())
    } else {
        workspace_folder_from_args(args)?
    };
    let container_id = resolve_target_container(
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
        .map(|value| &value.configuration)
        .or_else(|| inspected.as_ref().map(|value| &value.configuration));
    let interactive = common::has_flag(args, "--interactive");
    let remote_workspace_folder = resolved
        .as_ref()
        .map(remote_workspace_folder)
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

    let mut engine_args = vec!["exec".to_string()];
    if interactive {
        engine_args.push("-i".to_string());
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            engine_args.push("-t".to_string());
        }
    }
    engine_args.push("--workdir".to_string());
    engine_args.push(remote_workspace_folder);
    if let Some(user) = configuration.and_then(configured_user) {
        engine_args.push("--user".to_string());
        engine_args.push(user.to_string());
    }
    for (key, value) in combined_remote_env(args, configuration) {
        engine_args.push("-e".to_string());
        engine_args.push(format!("{key}={value}"));
    }
    engine_args.push(container_id);
    engine_args.extend(command_args);

    let request = ProcessRequest {
        program: engine_program(args),
        args: engine_args,
        cwd: None,
        env: std::collections::HashMap::new(),
    };

    if interactive {
        let status_code = process_runner::run_process_streaming(&request)?;
        Ok(ExecResult::Streaming(status_code))
    } else {
        process_runner::run_process(&request).map(ExecResult::Captured)
    }
}

struct ResolvedConfig {
    workspace_folder: PathBuf,
    config_file: PathBuf,
    configuration: Value,
}

struct InspectedContainerContext {
    configuration: Value,
    local_workspace_folder: Option<PathBuf>,
    remote_workspace_folder: Option<String>,
}

enum LifecycleMode {
    Up,
    SetUp,
    RunUserCommands,
}

enum LifecycleCommand {
    Shell(String),
    Exec(Vec<String>),
}

fn load_required_config(args: &[String]) -> Result<ResolvedConfig, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    Ok(ResolvedConfig {
        workspace_folder,
        config_file,
        configuration,
    })
}

fn load_optional_config(args: &[String]) -> Result<Option<ResolvedConfig>, String> {
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

fn engine_program(args: &[String]) -> String {
    common::parse_option_value(args, "--docker-path").unwrap_or_else(|| "docker".to_string())
}

fn runtime_image_name(resolved: &ResolvedConfig, args: &[String]) -> Result<String, String> {
    if has_build_definition(&resolved.configuration) {
        build_image(resolved, args)
    } else if let Some(image) = resolved.configuration.get("image").and_then(Value::as_str) {
        Ok(image.to_string())
    } else {
        Err(
            "Unsupported configuration: only image and build-based configs are supported natively"
                .to_string(),
        )
    }
}

fn build_image(resolved: &ResolvedConfig, args: &[String]) -> Result<String, String> {
    if !has_build_definition(&resolved.configuration) {
        return resolved
            .configuration
            .get("image")
            .and_then(Value::as_str)
            .map(|value| value.to_string())
            .ok_or_else(|| {
                "Unsupported configuration: only image and build-based configs are supported natively"
                    .to_string()
            });
    }

    let image_name = common::parse_option_value(args, "--image-name")
        .unwrap_or_else(|| default_image_name(&resolved.workspace_folder));
    let build = resolved
        .configuration
        .get("build")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let config_root = resolved
        .config_file
        .parent()
        .unwrap_or(resolved.workspace_folder.as_path());
    let dockerfile = build
        .get("dockerfile")
        .or_else(|| build.get("dockerFile"))
        .and_then(Value::as_str)
        .unwrap_or("Dockerfile");
    let context = build.get("context").and_then(Value::as_str).unwrap_or(".");
    let dockerfile_path = resolve_relative(config_root, dockerfile);
    let context_path = resolve_relative(config_root, context);

    let mut engine_args = vec![
        "build".to_string(),
        "--tag".to_string(),
        image_name.clone(),
        "--file".to_string(),
        dockerfile_path.display().to_string(),
    ];
    if common::has_flag(args, "--no-cache") || common::has_flag(args, "--build-no-cache") {
        engine_args.push("--no-cache".to_string());
    }
    for value in common::parse_option_values(args, "--cache-from") {
        engine_args.push("--cache-from".to_string());
        engine_args.push(value);
    }
    for value in common::parse_option_values(args, "--cache-to") {
        engine_args.push("--cache-to".to_string());
        engine_args.push(value);
    }
    for value in common::parse_option_values(args, "--label") {
        engine_args.push("--label".to_string());
        engine_args.push(value);
    }
    if let Some(build_args) = build.get("args").and_then(Value::as_object) {
        for (key, value) in build_args {
            if let Some(value) = value.as_str() {
                engine_args.push("--build-arg".to_string());
                engine_args.push(format!("{key}={value}"));
            }
        }
    }
    if let Some(platform) = common::parse_option_value(args, "--platform") {
        engine_args.push("--platform".to_string());
        engine_args.push(platform);
    }
    engine_args.push(context_path.display().to_string());

    let result = process_runner::run_process(&ProcessRequest {
        program: engine_program(args),
        args: engine_args,
        cwd: None,
        env: std::collections::HashMap::new(),
    })?;
    if result.status_code != 0 {
        return Err(stderr_or_stdout(&result));
    }

    if common::has_flag(args, "--push") {
        let push_result = process_runner::run_process(&ProcessRequest {
            program: engine_program(args),
            args: vec!["push".to_string(), image_name.clone()],
            cwd: None,
            env: std::collections::HashMap::new(),
        })?;
        if push_result.status_code != 0 {
            return Err(stderr_or_stdout(&push_result));
        }
    }

    Ok(image_name)
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

    let result = process_runner::run_process(&ProcessRequest {
        program: engine_program(args),
        args: engine_args,
        cwd: None,
        env: std::collections::HashMap::new(),
    })?;
    if result.status_code != 0 {
        return Err(stderr_or_stdout(&result));
    }

    let container_id = result.stdout.trim().to_string();
    if container_id.is_empty() {
        return Err("Container engine did not return a container id".to_string());
    }

    Ok(container_id)
}

fn ensure_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<String, String> {
    let existing = find_target_container(
        args,
        Some(resolved.workspace_folder.as_path()),
        Some(resolved.config_file.as_path()),
    )?;
    match existing {
        Some(container_id) if common::has_flag(args, "--remove-existing-container") => {
            remove_container(args, &container_id)?;
            start_container(resolved, args, image_name, remote_workspace_folder)
        }
        Some(container_id) => Ok(container_id),
        None if common::has_flag(args, "--expect-existing-container") => {
            Err("Dev container not found.".to_string())
        }
        None => start_container(resolved, args, image_name, remote_workspace_folder),
    }
}

fn remove_container(args: &[String], container_id: &str) -> Result<(), String> {
    let result = process_runner::run_process(&ProcessRequest {
        program: engine_program(args),
        args: vec!["rm".to_string(), "-f".to_string(), container_id.to_string()],
        cwd: None,
        env: std::collections::HashMap::new(),
    })?;
    if result.status_code != 0 {
        return Err(stderr_or_stdout(&result));
    }
    Ok(())
}

fn run_lifecycle_commands(
    container_id: &str,
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    mode: LifecycleMode,
) -> Result<(), String> {
    for command_group in selected_lifecycle_commands(configuration, args, mode) {
        if command_group.len() == 1 {
            run_lifecycle_command(
                container_id,
                args,
                configuration,
                remote_workspace_folder,
                command_group
                    .into_iter()
                    .next()
                    .expect("single lifecycle command"),
            )?;
            continue;
        }

        let handles = command_group
            .into_iter()
            .map(|command| {
                let request = lifecycle_exec_request(
                    container_id,
                    args,
                    configuration,
                    remote_workspace_folder,
                    command,
                );
                thread::spawn(move || process_runner::run_process(&request))
            })
            .collect::<Vec<_>>();

        let mut first_error = None;
        for handle in handles {
            match handle.join() {
                Ok(Ok(result)) if result.status_code == 0 => {}
                Ok(Ok(result)) => {
                    if first_error.is_none() {
                        first_error = Some(stderr_or_stdout(&result));
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
    }

    Ok(())
}

fn run_initialize_command(configuration: &Value, workspace_folder: &Path) -> Result<(), String> {
    let Some(command_group) = lifecycle_command_value(configuration, "initializeCommand") else {
        return Ok(());
    };

    if command_group.len() == 1 {
        return run_host_lifecycle_command(
            workspace_folder,
            command_group
                .into_iter()
                .next()
                .expect("single initialize command"),
        );
    }

    let handles = command_group
        .into_iter()
        .map(|command| {
            let request = host_lifecycle_request(workspace_folder, command);
            thread::spawn(move || process_runner::run_process(&request))
        })
        .collect::<Vec<_>>();

    let mut first_error = None;
    for handle in handles {
        match handle.join() {
            Ok(Ok(result)) if result.status_code == 0 => {}
            Ok(Ok(result)) => {
                if first_error.is_none() {
                    first_error = Some(stderr_or_stdout(&result));
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
                        Some("Initialize command thread panicked unexpectedly".to_string());
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

    match mode {
        LifecycleMode::Up | LifecycleMode::SetUp | LifecycleMode::RunUserCommands => {
            for (stage, command_group) in lifecycle_stages {
                if let Some(command_group) = command_group {
                    commands.push(command_group);
                }
                if skip_non_blocking && stage == wait_for {
                    break;
                }
            }
        }
    }

    commands
}

fn lifecycle_command_value(configuration: &Value, key: &str) -> Option<Vec<LifecycleCommand>> {
    let value = configuration.get(key)?;
    lifecycle_command_group(value)
}

fn lifecycle_command_group(value: &Value) -> Option<Vec<LifecycleCommand>> {
    match value {
        Value::String(command) => Some(vec![LifecycleCommand::Shell(command.clone())]),
        Value::Array(parts) => {
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
                Some(vec![LifecycleCommand::Exec(parts)])
            }
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
        Value::Array(parts) => {
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
                Some(LifecycleCommand::Exec(parts))
            }
        }
        _ => None,
    }
}

fn run_lifecycle_command(
    container_id: &str,
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    command: LifecycleCommand,
) -> Result<(), String> {
    let result = process_runner::run_process(&lifecycle_exec_request(
        container_id,
        args,
        configuration,
        remote_workspace_folder,
        command,
    ))?;
    if result.status_code != 0 {
        return Err(stderr_or_stdout(&result));
    }
    Ok(())
}

fn run_host_lifecycle_command(
    workspace_folder: &Path,
    command: LifecycleCommand,
) -> Result<(), String> {
    let result = process_runner::run_process(&host_lifecycle_request(workspace_folder, command))?;
    if result.status_code != 0 {
        return Err(stderr_or_stdout(&result));
    }
    Ok(())
}

fn lifecycle_exec_request(
    container_id: &str,
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    command: LifecycleCommand,
) -> ProcessRequest {
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

    ProcessRequest {
        program: engine_program(args),
        args: engine_args,
        cwd: None,
        env: std::collections::HashMap::new(),
    }
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

fn resolve_target_container(
    args: &[String],
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
) -> Result<String, String> {
    if let Some(container_id) = common::parse_option_value(args, "--container-id") {
        return Ok(container_id);
    }

    match find_target_container(args, workspace_folder, config_file)? {
        Some(container_id) => Ok(container_id),
        None => Err("Dev container not found.".to_string()),
    }
}

fn find_target_container(
    args: &[String],
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
) -> Result<Option<String>, String> {
    let labels = target_container_labels(args, workspace_folder, config_file);
    if labels.is_empty() {
        return Err(
            "Unable to determine target container. Provide --container-id or --workspace-folder."
                .to_string(),
        );
    }

    let mut engine_args = vec!["ps".to_string(), "-q".to_string()];
    for label in labels {
        engine_args.push("--filter".to_string());
        engine_args.push(format!("label={label}"));
    }

    let result = process_runner::run_process(&ProcessRequest {
        program: engine_program(args),
        args: engine_args,
        cwd: None,
        env: std::collections::HashMap::new(),
    })?;
    if result.status_code != 0 {
        return Err(stderr_or_stdout(&result));
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
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
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

fn inspect_container_context(
    args: &[String],
    container_id: &str,
) -> Result<InspectedContainerContext, String> {
    let result = process_runner::run_process(&ProcessRequest {
        program: engine_program(args),
        args: vec!["inspect".to_string(), container_id.to_string()],
        cwd: None,
        env: std::collections::HashMap::new(),
    })?;
    if result.status_code != 0 {
        return Err(stderr_or_stdout(&result));
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

fn merged_container_metadata(metadata_label: Option<&str>) -> Value {
    let Some(metadata_label) = metadata_label else {
        return Value::Object(Map::new());
    };
    let parsed = serde_json::from_str::<Value>(metadata_label).ok();
    let entries = match parsed {
        Some(Value::Array(values)) => values,
        Some(Value::Object(entries)) => vec![Value::Object(entries)],
        _ => Vec::new(),
    };

    let mut merged = Map::new();
    merge_last_metadata_value(&entries, &mut merged, "waitFor");
    merge_last_metadata_value(&entries, &mut merged, "workspaceFolder");
    merge_last_metadata_value(&entries, &mut merged, "remoteUser");
    merge_last_metadata_value(&entries, &mut merged, "containerUser");
    merge_object_metadata_value(&entries, &mut merged, "remoteEnv");
    merge_object_metadata_value(&entries, &mut merged, "containerEnv");

    for key in [
        "initializeCommand",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ] {
        if let Some(command_group) = merged_metadata_lifecycle_values(&entries, key) {
            merged.insert(key.to_string(), command_group);
        }
    }

    Value::Object(merged)
}

fn merge_last_metadata_value(entries: &[Value], merged: &mut Map<String, Value>, key: &str) {
    if let Some(value) = entries
        .iter()
        .filter_map(|entry| entry.get(key))
        .next_back()
    {
        merged.insert(key.to_string(), value.clone());
    }
}

fn merge_object_metadata_value(entries: &[Value], merged: &mut Map<String, Value>, key: &str) {
    let combined = entries
        .iter()
        .filter_map(|entry| entry.get(key).and_then(Value::as_object))
        .fold(Map::new(), |mut combined, value| {
            combined.extend(
                value
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone())),
            );
            combined
        });
    if !combined.is_empty() {
        merged.insert(key.to_string(), Value::Object(combined));
    }
}

fn merged_metadata_lifecycle_values(entries: &[Value], key: &str) -> Option<Value> {
    let commands = entries
        .iter()
        .filter_map(|entry| entry.get(key))
        .flat_map(flatten_lifecycle_values)
        .collect::<Vec<_>>();
    match commands.len() {
        0 => None,
        1 => commands.into_iter().next(),
        _ => Some(Value::Object(
            commands
                .into_iter()
                .enumerate()
                .map(|(index, value)| (index.to_string(), value))
                .collect(),
        )),
    }
}

fn flatten_lifecycle_values(value: &Value) -> Vec<Value> {
    match value {
        Value::String(_) | Value::Array(_) => vec![value.clone()],
        Value::Object(entries) => entries
            .values()
            .flat_map(flatten_lifecycle_values)
            .collect(),
        _ => Vec::new(),
    }
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

fn default_remote_workspace_folder(workspace_folder: Option<&Path>) -> String {
    let basename = workspace_folder
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .unwrap_or("workspace");
    format!("/workspaces/{basename}")
}

fn remote_workspace_folder(resolved: &ResolvedConfig) -> String {
    resolved
        .configuration
        .get("workspaceFolder")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| default_remote_workspace_folder(Some(&resolved.workspace_folder)))
}

fn workspace_mount(resolved: &ResolvedConfig, remote_workspace_folder: &str) -> String {
    resolved
        .configuration
        .get("workspaceMount")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "type=bind,source={},target={remote_workspace_folder}",
                resolved.workspace_folder.display()
            )
        })
}

fn default_image_name(workspace_folder: &Path) -> String {
    let basename = workspace_folder
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("devcontainer-{basename}")
}

fn remote_user(configuration: &Value) -> String {
    configured_user(configuration).unwrap_or("root").to_string()
}

fn configured_user(configuration: &Value) -> Option<&str> {
    configuration
        .get("remoteUser")
        .or_else(|| configuration.get("containerUser"))
        .and_then(Value::as_str)
}

fn has_build_definition(configuration: &Value) -> bool {
    configuration
        .get("build")
        .is_some_and(|value| value.is_object())
}

fn resolve_relative(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn exec_command_and_args(args: &[String]) -> Result<Vec<String>, String> {
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if matches!(
            arg.as_str(),
            "--docker-path"
                | "--workspace-folder"
                | "--config"
                | "--remote-env"
                | "--container-id"
                | "--id-label"
        ) {
            index += 2;
            continue;
        }
        if arg == "--interactive" {
            index += 1;
            continue;
        }
        if arg.starts_with("--") {
            return Err(format!("Unsupported exec option: {arg}"));
        }
        break;
    }

    if index >= args.len() {
        return Err("exec requires a command to run".to_string());
    }

    Ok(args[index..].to_vec())
}

fn combined_remote_env(
    args: &[String],
    configuration: Option<&Value>,
) -> std::collections::HashMap<String, String> {
    let mut remote_env = std::collections::HashMap::new();
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
    remote_env.extend(common::remote_env_overrides(args));
    remote_env
}

fn stderr_or_stdout(result: &ProcessResult) -> String {
    if result.stderr.trim().is_empty() {
        result.stdout.trim().to_string()
    } else {
        result.stderr.trim().to_string()
    }
}
