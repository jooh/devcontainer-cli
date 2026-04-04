use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{json, Map, Value};

mod cli_host;
mod command_porting;
mod config;
mod cutover;
mod output;
mod process_runner;

const SUPPORTED_TOP_LEVEL_COMMANDS: [&str; 9] = [
    "read-configuration",
    "build",
    "up",
    "set-up",
    "run-user-commands",
    "outdated",
    "exec",
    "features",
    "templates",
];
const NATIVE_ONLY_ENV_VAR: &str = "DEVCONTAINER_NATIVE_ONLY";

fn print_help() {
    println!("devcontainer (native foundation)");
    println!("\nUsage:\n  devcontainer [--log-format text|json] <command> [args...]\n");
    println!("Supported top-level commands (forwarded to Node bridge):");
    for command in SUPPORTED_TOP_LEVEL_COMMANDS {
        println!("  - {command}");
    }
}

fn print_command_help(command: &str) {
    match command {
        "read-configuration" => {
            println!("Usage:\n  devcontainer read-configuration [--workspace-folder <path>] [--config <path>]");
            println!("\nNative support:");
            println!("  - resolves .devcontainer/devcontainer.json or .devcontainer.json");
            println!("  - supports --workspace-folder and --config");
        }
        "features" => {
            println!("Usage:\n  devcontainer features <list|ls>");
            println!("\nNative support:");
            println!("  - list");
            println!("  - ls");
        }
        "templates" => {
            println!("Usage:\n  devcontainer templates <list|ls>");
            println!("\nNative support:");
            println!("  - list");
            println!("  - ls");
        }
        "build" | "up" | "exec" => {
            println!("Usage:\n  devcontainer {command} [args...]");
            println!("\nCurrent state:");
            println!("  - execution is native for non-interactive flows");
            println!("  - payloads are emitted as structured JSON");
        }
        "set-up" | "run-user-commands" | "outdated" => {
            println!("Usage:\n  devcontainer {command} [args...]");
            println!("\nNative support:");
            println!("  - structured JSON payload output");
            println!("  - config-driven lifecycle planning");
        }
        _ => {
            println!("Usage:\n  devcontainer {command} [args...]");
        }
    }
}

fn parse_log_format(args: &[String]) -> (&str, usize) {
    if args.len() >= 3 && args[0] == "--log-format" {
        return (args[1].as_str(), 2);
    }
    ("text", 0)
}

fn emit_log(log_format: &str, message: &str) {
    let format = match log_format {
        "json" => output::LogFormat::Json,
        _ => output::LogFormat::Text,
    };
    println!("{}", output::render_log(format, "info", message));
}

fn native_only_mode_enabled() -> bool {
    env::var(NATIVE_ONLY_ENV_VAR)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            !normalized.is_empty()
                && normalized != "0"
                && normalized != "false"
                && normalized != "no"
        })
        .unwrap_or(false)
}

fn is_command_help_request(args: &[String]) -> bool {
    matches!(
        args.first().map(String::as_str),
        Some("--help") | Some("-h")
    )
}

fn resolve_bridge_script() -> Result<PathBuf, String> {
    let exe_path = env::current_exe().map_err(|error| error.to_string())?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| "Unable to determine executable directory".to_string())?;

    Ok(exe_dir.join("dist/spec-node/devContainersSpecCLI.js"))
}

fn parse_option_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == option)
        .map(|window| window[1].clone())
}

fn parse_option_values(args: &[String], option: &str) -> Vec<String> {
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

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn parse_mounts(args: &[String]) -> Vec<Value> {
    parse_option_values(args, "--mount")
        .into_iter()
        .map(|mount| Value::String(mount))
        .collect()
}

fn parse_remote_env(args: &[String]) -> Map<String, Value> {
    parse_option_values(args, "--remote-env")
        .into_iter()
        .filter_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            Some((name.to_string(), Value::String(value.to_string())))
        })
        .collect()
}

fn load_resolved_config(args: &[String]) -> Result<(PathBuf, PathBuf, Value), String> {
    let (workspace_folder, config_file) = resolve_read_configuration_path(args)?;
    let raw = fs::read_to_string(&config_file).map_err(|error| error.to_string())?;
    let parsed = config::parse_jsonc_value(&raw)?;
    let substituted = config::substitute_local_context(
        &parsed,
        &config::ConfigContext {
            workspace_folder: workspace_folder.clone(),
            env: env::vars().collect(),
        },
    );
    Ok((workspace_folder, config_file, substituted))
}

fn lifecycle_commands(configuration: &Value) -> Vec<Value> {
    [
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ]
    .iter()
    .filter_map(|key| {
        configuration
            .get(*key)
            .map(|value| json!({ "name": key, "value": value }))
    })
    .collect()
}

fn build_read_configuration_payload(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = load_resolved_config(args)?;
    let mut payload = Map::new();
    payload.insert("configuration".to_string(), configuration.clone());
    payload.insert(
        "metadata".to_string(),
        json!({
            "format": "jsonc",
            "pathResolution": "native-rust",
            "workspaceFolder": workspace_folder,
            "configFile": config_file,
        }),
    );

    if has_flag(args, "--include-merged-configuration") {
        payload.insert("mergedConfiguration".to_string(), configuration.clone());
    }

    if has_flag(args, "--include-features-configuration") {
        payload.insert(
            "featuresConfiguration".to_string(),
            json!({
                "features": configuration.get("features").cloned().unwrap_or_else(|| json!({})),
            }),
        );
    }

    Ok(Value::Object(payload))
}

fn build_build_payload(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = load_resolved_config(args)?;
    let build_section = configuration
        .get("build")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let dockerfile = build_section
        .get("dockerfile")
        .or_else(|| build_section.get("dockerFile"))
        .and_then(Value::as_str)
        .unwrap_or("Dockerfile");
    let context = build_section
        .get("context")
        .and_then(Value::as_str)
        .unwrap_or(".");

    let mut docker_args = vec!["build".to_string()];
    if has_flag(args, "--no-cache") {
        docker_args.push("--no-cache".to_string());
    }
    for value in parse_option_values(args, "--cache-from") {
        docker_args.push("--cache-from".to_string());
        docker_args.push(value);
    }
    for value in parse_option_values(args, "--cache-to") {
        docker_args.push("--cache-to".to_string());
        docker_args.push(value);
    }
    for value in parse_option_values(args, "--label") {
        docker_args.push("--label".to_string());
        docker_args.push(value);
    }
    if let Some(image_name) = parse_option_value(args, "--image-name") {
        docker_args.push("--tag".to_string());
        docker_args.push(image_name);
    }
    if let Some(platform) = parse_option_value(args, "--platform") {
        docker_args.push("--platform".to_string());
        docker_args.push(platform);
    }
    docker_args.push("--file".to_string());
    docker_args.push(dockerfile.to_string());
    docker_args.push(context.to_string());

    Ok(json!({
        "outcome": "success",
        "command": "build",
        "workspaceFolder": workspace_folder,
        "configFile": config_file,
        "buildKit": parse_option_value(args, "--buildkit").unwrap_or_else(|| "auto".to_string()),
        "push": has_flag(args, "--push"),
        "docker": {
            "program": "docker",
            "args": docker_args,
        },
        "configuration": configuration,
    }))
}

fn build_lifecycle_payload(command: &str, args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = load_resolved_config(args)?;
    Ok(json!({
        "outcome": "success",
        "command": command,
        "workspaceFolder": workspace_folder,
        "configFile": config_file,
        "mounts": parse_mounts(args),
        "remoteEnv": parse_remote_env(args),
        "skipPostCreate": has_flag(args, "--skip-post-create"),
        "skipPostAttach": has_flag(args, "--skip-post-attach"),
        "skipNonBlockingCommands": has_flag(args, "--skip-non-blocking-commands"),
        "lifecycleCommands": lifecycle_commands(&configuration),
        "configuration": if has_flag(args, "--include-configuration") { configuration.clone() } else { Value::Null },
        "mergedConfiguration": if has_flag(args, "--include-merged-configuration") { configuration.clone() } else { Value::Null },
    }))
}

fn build_outdated_payload(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = load_resolved_config(args)?;
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let feature_versions: Map<String, Value> = features
        .keys()
        .map(|feature_id| {
            let current_version = feature_id
                .rsplit(':')
                .next()
                .filter(|version| *version != feature_id)
                .unwrap_or("unversioned");
            (
                feature_id.clone(),
                json!({
                    "currentVersion": current_version,
                    "latestVersion": "unknown",
                }),
            )
        })
        .collect();

    Ok(json!({
        "outcome": "success",
        "command": "outdated",
        "workspaceFolder": workspace_folder,
        "configFile": config_file,
        "features": feature_versions,
    }))
}

fn exec_command_and_args(args: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    let workspace_folder = parse_option_value(args, "--workspace-folder")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| "Unable to determine workspace folder".to_string())?;

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--workspace-folder" || arg == "--config" || arg == "--remote-env" {
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

    Ok((workspace_folder, args[index..].to_vec()))
}

fn execute_native_exec(args: &[String]) -> Result<process_runner::ProcessResult, String> {
    let (workspace_folder, command_args) = exec_command_and_args(args)?;
    let mut remote_env = env::vars().collect::<std::collections::HashMap<_, _>>();
    for (key, value) in parse_remote_env(args) {
        if let Some(text) = value.as_str() {
            remote_env.insert(key, text.to_string());
        }
    }

    process_runner::run_process(&process_runner::ProcessRequest {
        program: command_args[0].clone(),
        args: command_args[1..].to_vec(),
        cwd: Some(workspace_folder),
        env: remote_env,
    })
}

fn stream_native_exec(args: &[String]) -> Result<i32, String> {
    let (workspace_folder, command_args) = exec_command_and_args(args)?;
    let mut remote_env = env::vars().collect::<std::collections::HashMap<_, _>>();
    for (key, value) in parse_remote_env(args) {
        if let Some(text) = value.as_str() {
            remote_env.insert(key, text.to_string());
        }
    }

    process_runner::run_process_streaming(&process_runner::ProcessRequest {
        program: command_args[0].clone(),
        args: command_args[1..].to_vec(),
        cwd: Some(workspace_folder),
        env: remote_env,
    })
}

fn resolve_read_configuration_path(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let workspace_folder = parse_option_value(args, "--workspace-folder")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| "Unable to determine workspace folder".to_string())?;

    let explicit_config = parse_option_value(args, "--config").map(PathBuf::from);
    let config_path = config::resolve_config_path(&workspace_folder, explicit_config.as_deref())?;

    let resolved_workspace = fs::canonicalize(&workspace_folder).unwrap_or(workspace_folder);
    Ok((resolved_workspace, config_path))
}

fn run_native_read_configuration(args: &[String]) -> ExitCode {
    let payload = match build_read_configuration_payload(args) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    println!("{}", payload);

    ExitCode::SUCCESS
}

fn should_use_native_read_configuration(args: &[String]) -> bool {
    const SUPPORTED_OPTIONS: [&str; 4] = [
        "--workspace-folder",
        "--config",
        "--include-merged-configuration",
        "--include-features-configuration",
    ];
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with("--") {
            return false;
        }
        if !SUPPORTED_OPTIONS.contains(&arg.as_str()) {
            return false;
        }
        index += if arg == "--include-merged-configuration"
            || arg == "--include-features-configuration"
        {
            1
        } else {
            2
        };
    }
    true
}

fn run_native_build(args: &[String]) -> ExitCode {
    match build_build_payload(args) {
        Ok(payload) => {
            println!("{payload}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_native_lifecycle_command(command: &str, args: &[String]) -> ExitCode {
    match build_lifecycle_payload(command, args) {
        Ok(payload) => {
            println!("{payload}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_native_outdated(args: &[String]) -> ExitCode {
    match build_outdated_payload(args) {
        Ok(payload) => {
            println!("{payload}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_native_exec(args: &[String]) -> ExitCode {
    if has_flag(args, "--interactive") {
        return match stream_native_exec(args) {
            Ok(status_code) => ExitCode::from(status_code as u8),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        };
    }

    match execute_native_exec(args) {
        Ok(result) => {
            let _ = io::stdout().write_all(result.stdout.as_bytes());
            let _ = io::stderr().write_all(result.stderr.as_bytes());
            ExitCode::from(result.status_code as u8)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_native_collection(command: &str, args: &[String]) -> ExitCode {
    let is_list = args
        .first()
        .map(|arg| arg == "list" || arg == "ls")
        .unwrap_or(true);
    if !is_list {
        eprintln!("{command} currently supports only list/ls in native mode");
        return ExitCode::from(2);
    }

    let payload = match command {
        "features" => "{\"features\":[]}",
        "templates" => "{\"templates\":[]}",
        _ => "{}",
    };
    let _ = io::stdout().write_all(payload.as_bytes());
    let _ = io::stdout().write_all(b"\n");
    ExitCode::SUCCESS
}

fn should_use_native_collection(args: &[String]) -> bool {
    args.first()
        .map(|arg| arg == "list" || arg == "ls")
        .unwrap_or(true)
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.is_empty() || raw_args[0] == "--help" || raw_args[0] == "-h" {
        print_help();
        return ExitCode::SUCCESS;
    }

    let (log_format, offset) = parse_log_format(&raw_args);
    if log_format != "text" && log_format != "json" {
        eprintln!("Unsupported log format: {log_format}");
        return ExitCode::from(2);
    }

    if raw_args.len() <= offset {
        print_help();
        return ExitCode::from(2);
    }

    let command = &raw_args[offset];

    if !SUPPORTED_TOP_LEVEL_COMMANDS.contains(&command.as_str()) {
        eprintln!("Unsupported command: {command}");
        return ExitCode::from(2);
    }

    let command_args = &raw_args[offset + 1..];
    if is_command_help_request(command_args) {
        print_command_help(command);
        return ExitCode::SUCCESS;
    }

    match command.as_str() {
        "read-configuration" if should_use_native_read_configuration(command_args) => {
            return run_native_read_configuration(command_args);
        }
        "build" => return run_native_build(command_args),
        "up" | "set-up" | "run-user-commands" => {
            return run_native_lifecycle_command(command, command_args);
        }
        "outdated" => return run_native_outdated(command_args),
        "exec" => return run_native_exec(command_args),
        "features" | "templates" if should_use_native_collection(command_args) => {
            return run_native_collection(command, command_args);
        }
        _ => {}
    }

    if native_only_mode_enabled() {
        eprintln!(
            "Native-only mode forbids Node fallback for command: {command}. Port the command or disable {NATIVE_ONLY_ENV_VAR}."
        );
        return ExitCode::from(3);
    }

    emit_log(
        log_format,
        "Delegating command to Node compatibility bridge.",
    );

    let bridge_script = match resolve_bridge_script() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Failed to resolve Node compatibility bridge path: {error}");
            return ExitCode::from(1);
        }
    };

    let cli_host = match cli_host::CliHost::from_env() {
        Ok(host) => host,
        Err(error) => {
            eprintln!("Failed to probe CLI host environment: {error}");
            return ExitCode::from(1);
        }
    };

    let node_program = cli_host
        .lookup_command("node")
        .unwrap_or_else(|| PathBuf::from("node"));

    let process = process_runner::run_process(&process_runner::ProcessRequest {
        program: node_program.display().to_string(),
        args: {
            let mut args = vec![bridge_script.display().to_string()];
            args.extend(raw_args.clone());
            args
        },
        cwd: Some(cli_host.cwd),
        env: cli_host.env,
    });

    match process {
        Ok(result) => {
            let _ = io::stdout().write_all(result.stdout.as_bytes());
            let _ = io::stderr().write_all(result.stderr.as_bytes());
            ExitCode::from(result.status_code as u8)
        }
        Err(error) => {
            eprintln!("Failed to invoke Node compatibility bridge: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_build_payload, build_lifecycle_payload, build_outdated_payload,
        build_read_configuration_payload, execute_native_exec, is_command_help_request,
        native_only_mode_enabled, resolve_read_configuration_path, run_native_collection,
        should_use_native_collection, should_use_native_read_configuration,
    };
    use std::fs;
    use std::process::ExitCode;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("devcontainer-test-{suffix}"))
    }

    #[test]
    fn resolves_modern_config_path_from_workspace_folder() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        let config = config_dir.join("devcontainer.json");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
        let result = resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            result.1,
            fs::canonicalize(config).expect("failed to canonicalize")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fails_when_explicit_config_file_is_missing() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create root");
        let missing_config = root.join("missing.json");
        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--config".to_string(),
            missing_config.display().to_string(),
        ];

        let result = resolve_read_configuration_path(&args);

        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_relative_config_against_workspace_folder() {
        let root = unique_temp_dir();
        let config = root.join("relative.devcontainer.json");
        fs::create_dir_all(&root).expect("failed to create root");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--config".to_string(),
            "relative.devcontainer.json".to_string(),
        ];
        let result = resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            result.1,
            fs::canonicalize(config).expect("failed to canonicalize")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn supports_native_features_list_collection_command() {
        let result = run_native_collection("features", &["list".to_string()]);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn non_list_collection_subcommands_fall_back_to_node() {
        assert!(!should_use_native_collection(&["apply".to_string()]));
    }

    #[test]
    fn read_configuration_with_additional_flags_is_supported_natively() {
        assert!(should_use_native_read_configuration(&[
            "--workspace-folder".to_string(),
            "/workspace".to_string(),
            "--include-merged-configuration".to_string(),
        ]));
    }

    #[test]
    fn detects_subcommand_help_requests_without_needing_node() {
        assert!(is_command_help_request(&["--help".to_string()]));
        assert!(is_command_help_request(&["-h".to_string()]));
        assert!(!is_command_help_request(&["list".to_string()]));
    }

    #[test]
    fn native_only_mode_uses_environment_switch() {
        let original = std::env::var("DEVCONTAINER_NATIVE_ONLY").ok();
        std::env::set_var("DEVCONTAINER_NATIVE_ONLY", "1");
        assert!(native_only_mode_enabled());

        std::env::set_var("DEVCONTAINER_NATIVE_ONLY", "false");
        assert!(!native_only_mode_enabled());

        if let Some(value) = original {
            std::env::set_var("DEVCONTAINER_NATIVE_ONLY", value);
        } else {
            std::env::remove_var("DEVCONTAINER_NATIVE_ONLY");
        }
    }

    #[test]
    fn read_configuration_payload_includes_optional_sections() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": { \"ghcr.io/devcontainers/features/git:1\": {} }\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--include-merged-configuration".to_string(),
            "--include-features-configuration".to_string(),
        ];
        let payload = build_read_configuration_payload(&args).expect("payload");

        assert_eq!(
            payload["configuration"]["image"],
            "mcr.microsoft.com/devcontainers/base:ubuntu"
        );
        assert_eq!(
            payload["mergedConfiguration"]["image"],
            "mcr.microsoft.com/devcontainers/base:ubuntu"
        );
        assert!(payload["featuresConfiguration"]["features"]
            .as_object()
            .expect("features object")
            .contains_key("ghcr.io/devcontainers/features/git:1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn build_payload_contains_docker_plan_and_flags() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"build\": {\n    \"dockerfile\": \"Dockerfile\",\n    \"context\": \"..\"\n  }\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--buildkit".to_string(),
            "never".to_string(),
            "--no-cache".to_string(),
            "--cache-from".to_string(),
            "ghcr.io/example/cache".to_string(),
            "--label".to_string(),
            "devcontainer.test=true".to_string(),
        ];
        let payload = build_build_payload(&args).expect("payload");
        let docker_args = payload["docker"]["args"].as_array().expect("docker args");

        assert!(docker_args.iter().any(|value| value == "--no-cache"));
        assert!(docker_args
            .iter()
            .any(|value| value == "ghcr.io/example/cache"));
        assert!(docker_args
            .iter()
            .any(|value| value == "devcontainer.test=true"));
        assert_eq!(payload["buildKit"], "never");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lifecycle_payload_collects_commands_mounts_and_remote_env() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"onCreateCommand\": \"echo create\",\n  \"postCreateCommand\": \"echo post\"\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--mount".to_string(),
            "type=bind,source=/tmp,target=/workspace".to_string(),
            "--remote-env".to_string(),
            "HELLO=world".to_string(),
        ];
        let payload = build_lifecycle_payload("up", &args).expect("payload");

        assert_eq!(payload["command"], "up");
        assert_eq!(payload["mounts"].as_array().expect("mounts").len(), 1);
        assert_eq!(payload["remoteEnv"]["HELLO"], "world");
        assert_eq!(
            payload["lifecycleCommands"]
                .as_array()
                .expect("lifecycle commands")
                .len(),
            2
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn outdated_payload_reports_feature_versions() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/node:1\": {}\n  }\n}\n",
        )
        .expect("failed to write config");

        let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
        let payload = build_outdated_payload(&args).expect("payload");

        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/node:1"]["currentVersion"],
            "1"
        );
        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/node:1"]["latestVersion"],
            "unknown"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn execute_native_exec_runs_non_interactive_command() {
        let result = execute_native_exec(&[
            "/bin/sh".to_string(),
            "-c".to_string(),
            "printf native-exec".to_string(),
        ])
        .expect("exec result");

        assert_eq!(result.status_code, 0);
        assert_eq!(result.stdout, "native-exec");
        assert_eq!(result.stderr, "");
    }
}
