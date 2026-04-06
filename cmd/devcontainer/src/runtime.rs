use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::commands::common;
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
    let image_name = runtime_image_name(&resolved, args)?;
    let remote_workspace_folder = remote_workspace_folder(&resolved);
    let container_id = start_container(&resolved, args, &image_name, &remote_workspace_folder)?;
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
    let remote_workspace_folder = resolved
        .as_ref()
        .map(remote_workspace_folder)
        .unwrap_or_else(|| default_remote_workspace_folder(fallback_workspace.as_deref()));

    run_lifecycle_commands(
        &container_id,
        args,
        resolved
            .as_ref()
            .map(|value| &value.configuration)
            .unwrap_or(&Value::Object(Map::new())),
        &remote_workspace_folder,
        LifecycleMode::SetUp,
    )?;

    let configuration = resolved
        .as_ref()
        .map(|value| value.configuration.clone())
        .unwrap_or_else(|| json!({}));

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
    let remote_workspace_folder = resolved
        .as_ref()
        .map(remote_workspace_folder)
        .unwrap_or_else(|| default_remote_workspace_folder(fallback_workspace.as_deref()));

    run_lifecycle_commands(
        &container_id,
        args,
        resolved
            .as_ref()
            .map(|value| &value.configuration)
            .unwrap_or(&Value::Object(Map::new())),
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
    let remote_workspace_folder = resolved
        .as_ref()
        .map(remote_workspace_folder)
        .unwrap_or_else(|| default_remote_workspace_folder(workspace_folder.as_deref()));
    let container_id = resolve_target_container(
        args,
        resolved
            .as_ref()
            .map(|value| value.workspace_folder.as_path())
            .or(workspace_folder.as_deref()),
        resolved.as_ref().map(|value| value.config_file.as_path()),
    )?;
    let interactive = common::has_flag(args, "--interactive");

    let mut engine_args = vec!["exec".to_string()];
    if interactive {
        engine_args.push("-i".to_string());
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            engine_args.push("-t".to_string());
        }
    }
    engine_args.push("--workdir".to_string());
    engine_args.push(remote_workspace_folder);
    if let Some(user) = resolved
        .as_ref()
        .and_then(|value| configured_user(&value.configuration))
    {
        engine_args.push("--user".to_string());
        engine_args.push(user.to_string());
    }
    for (key, value) in
        combined_remote_env(args, resolved.as_ref().map(|value| &value.configuration))
    {
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

enum LifecycleMode {
    Up,
    SetUp,
    RunUserCommands,
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
    let explicit_workspace = common::parse_option_value(args, "--workspace-folder");
    if explicit_config.is_none() && explicit_workspace.is_none() {
        return Ok(None);
    }

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
    if common::has_flag(args, "--no-cache") {
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

fn run_lifecycle_commands(
    container_id: &str,
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    mode: LifecycleMode,
) -> Result<(), String> {
    for command in selected_lifecycle_commands(configuration, args, mode) {
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
        engine_args.push("sh".to_string());
        engine_args.push("-lc".to_string());
        engine_args.push(command);

        let result = process_runner::run_process(&ProcessRequest {
            program: engine_program(args),
            args: engine_args,
            cwd: None,
            env: std::collections::HashMap::new(),
        })?;
        if result.status_code != 0 {
            return Err(stderr_or_stdout(&result));
        }
    }

    Ok(())
}

fn selected_lifecycle_commands(
    configuration: &Value,
    args: &[String],
    mode: LifecycleMode,
) -> Vec<String> {
    let skip_post_create = common::has_flag(args, "--skip-post-create");
    let skip_post_attach = common::has_flag(args, "--skip-post-attach");
    let skip_non_blocking = common::has_flag(args, "--skip-non-blocking-commands");

    let mut commands = Vec::new();
    let include_create = !skip_post_create;

    if include_create {
        commands.extend(lifecycle_command_values(
            configuration,
            &[
                "onCreateCommand",
                "updateContentCommand",
                "postCreateCommand",
            ],
        ));
    }
    commands.extend(lifecycle_command_values(
        configuration,
        &["postStartCommand"],
    ));
    if !skip_post_attach {
        commands.extend(lifecycle_command_values(
            configuration,
            &["postAttachCommand"],
        ));
    }

    if skip_non_blocking {
        match mode {
            LifecycleMode::Up | LifecycleMode::SetUp | LifecycleMode::RunUserCommands => {
                commands.truncate(commands.len().min(3));
            }
        }
    }

    commands
}

fn lifecycle_command_values(configuration: &Value, keys: &[&str]) -> Vec<String> {
    let mut commands = Vec::new();
    for key in keys {
        let Some(value) = configuration.get(*key) else {
            continue;
        };
        match value {
            Value::String(command) => commands.push(command.clone()),
            Value::Array(parts) => {
                let joined = parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                if !joined.is_empty() {
                    commands.push(joined);
                }
            }
            _ => {}
        }
    }
    commands
}

fn resolve_target_container(
    args: &[String],
    workspace_folder: Option<&Path>,
    config_file: Option<&Path>,
) -> Result<String, String> {
    if let Some(container_id) = common::parse_option_value(args, "--container-id") {
        return Ok(container_id);
    }

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

    let container_id = result
        .stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if container_id.is_empty() {
        return Err("Dev container not found.".to_string());
    }

    Ok(container_id)
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
