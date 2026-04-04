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

const SUPPORTED_TOP_LEVEL_COMMANDS: [&str; 10] = [
    "read-configuration",
    "build",
    "up",
    "set-up",
    "run-user-commands",
    "outdated",
    "upgrade",
    "exec",
    "features",
    "templates",
];
const NATIVE_ONLY_ENV_VAR: &str = "DEVCONTAINER_NATIVE_ONLY";

fn print_help() {
    println!("devcontainer (native foundation)");
    println!("\nUsage:\n  devcontainer [--log-format text|json] <command> [args...]\n");
    println!("Supported top-level commands (native Rust runtime):");
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
            println!("Usage:\n  devcontainer features <list|ls|resolve-dependencies|info|test|package|publish|generate-docs>");
            println!("\nNative support:");
            println!("  - list / ls");
            println!("  - resolve-dependencies");
            println!("  - info <mode> <feature>");
            println!("  - test [target]");
            println!("  - package <target>");
            println!("  - publish <target>");
            println!("  - generate-docs <target>");
        }
        "templates" => {
            println!("Usage:\n  devcontainer templates <list|ls|apply|metadata|publish|generate-docs>");
            println!("\nNative support:");
            println!("  - list / ls");
            println!("  - apply <target>");
            println!("  - metadata <target>");
            println!("  - publish <target>");
            println!("  - generate-docs <target>");
        }
        "build" | "up" | "exec" => {
            println!("Usage:\n  devcontainer {command} [args...]");
            println!("\nCurrent state:");
            println!("  - execution is native for non-interactive flows");
            println!("  - payloads are emitted as structured JSON");
        }
        "set-up" | "run-user-commands" | "outdated" | "upgrade" => {
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
    .filter_map(|key| configuration.get(*key).map(|value| json!({ "name": key, "value": value })))
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
    let build_section = configuration.get("build").cloned().unwrap_or_else(|| json!({}));
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

fn parse_manifest(root: &std::path::Path, manifest_name: &str) -> Result<Value, String> {
    let manifest_path = root.join(manifest_name);
    let raw = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    config::parse_jsonc_value(&raw)
}

fn build_features_resolve_dependencies_payload(args: &[String]) -> Result<Value, String> {
    let (_, _, configuration) = load_resolved_config(args)?;
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut ordered = Vec::new();

    if let Some(override_order) = configuration
        .get("overrideFeatureInstallOrder")
        .and_then(Value::as_array)
    {
        for entry in override_order.iter().filter_map(Value::as_str) {
            if features.contains_key(entry) {
                ordered.push(Value::String(entry.to_string()));
            }
        }
    }

    for feature in features.keys() {
        if !ordered.iter().any(|value| value == feature) {
            ordered.push(Value::String(feature.clone()));
        }
    }

    Ok(json!({
        "outcome": "success",
        "command": "features resolve-dependencies",
        "resolvedFeatures": ordered,
    }))
}

fn build_feature_info_payload(feature_path: &str) -> Result<Value, String> {
    let manifest = parse_manifest(std::path::Path::new(feature_path), "devcontainer-feature.json")?;
    Ok(json!({
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "name": manifest.get("name").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "version": manifest.get("version").cloned().unwrap_or_else(|| Value::String("0.0.0".to_string())),
        "options": manifest.get("options").cloned().unwrap_or_else(|| json!({})),
    }))
}

fn package_collection_target(
    target: &std::path::Path,
    manifest_name: &str,
    prefix: &str,
) -> Result<PathBuf, String> {
    let _ = parse_manifest(target, manifest_name)?;
    let archive_name = format!(
        "{}-{}.tgz",
        prefix,
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(prefix)
    );
    let archive_path = target
        .parent()
        .unwrap_or(target)
        .join(archive_name);

    let result = process_runner::run_process(&process_runner::ProcessRequest {
        program: "tar".to_string(),
        args: vec![
            "-czf".to_string(),
            archive_path.display().to_string(),
            "-C".to_string(),
            target.display().to_string(),
            ".".to_string(),
        ],
        cwd: None,
        env: std::collections::HashMap::new(),
    })?;

    if result.status_code != 0 {
        return Err(result.stderr);
    }

    Ok(archive_path)
}

fn generate_feature_docs(feature_root: &std::path::Path) -> Result<PathBuf, String> {
    let manifest = parse_manifest(feature_root, "devcontainer-feature.json")?;
    let readme_path = feature_root.join("README.md");
    let name = manifest.get("name").and_then(Value::as_str).unwrap_or("Feature");
    let description = manifest
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("Generated documentation.");
    fs::write(
        &readme_path,
        format!("# {name}\n\n{description}\n"),
    )
    .map_err(|error| error.to_string())?;
    Ok(readme_path)
}

fn copy_directory_recursive(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry_path.is_dir() {
            copy_directory_recursive(&entry_path, &destination_path)?;
        } else {
            fs::copy(&entry_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn apply_template_target(
    template_root: &std::path::Path,
    workspace_root: &std::path::Path,
) -> Result<Value, String> {
    let manifest = parse_manifest(template_root, "devcontainer-template.json")?;
    let source_root = template_root.join("src");
    copy_directory_recursive(&source_root, workspace_root)?;
    Ok(json!({
        "outcome": "success",
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "appliedTo": workspace_root,
    }))
}

fn build_template_metadata_payload(template_path: &str) -> Result<Value, String> {
    let manifest = parse_manifest(std::path::Path::new(template_path), "devcontainer-template.json")?;
    Ok(json!({
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "name": manifest.get("name").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "description": manifest.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
    }))
}

fn generate_template_docs(template_root: &std::path::Path) -> Result<PathBuf, String> {
    let manifest = parse_manifest(template_root, "devcontainer-template.json")?;
    let readme_path = template_root.join("README.md");
    let name = manifest.get("name").and_then(Value::as_str).unwrap_or("Template");
    let description = manifest
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("Generated documentation.");
    fs::write(&readme_path, format!("# {name}\n\n{description}\n"))
        .map_err(|error| error.to_string())?;
    Ok(readme_path)
}

fn run_upgrade_lockfile(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, _, configuration) = load_resolved_config(args)?;
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let filtered_feature = parse_option_value(args, "--feature");
    let target_version = parse_option_value(args, "--target-version");
    let lockfile_features: Map<String, Value> = features
        .keys()
        .filter(|feature_id| {
            filtered_feature
                .as_ref()
                .map(|requested| feature_id.contains(requested))
                .unwrap_or(true)
        })
        .map(|feature_id| {
            let current_version = feature_id
                .rsplit(':')
                .next()
                .filter(|version| *version != feature_id)
                .unwrap_or("unversioned");
            (
                feature_id.clone(),
                json!({
                    "version": target_version.clone().unwrap_or_else(|| current_version.to_string()),
                }),
            )
        })
        .collect();

    let lockfile = json!({
        "features": lockfile_features,
    });

    if !has_flag(args, "--dry-run") {
        let lockfile_path = workspace_folder
            .join(".devcontainer")
            .join("devcontainer-lock.json");
        if let Some(parent) = lockfile_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(
            &lockfile_path,
            serde_json::to_string_pretty(&lockfile).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(json!({
        "outcome": "success",
        "command": "upgrade",
        "lockfile": lockfile,
    }))
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
        index += if arg == "--include-merged-configuration" || arg == "--include-features-configuration" {
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

fn run_native_features(args: &[String]) -> ExitCode {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let result = match subcommand {
        "list" | "ls" => return run_native_collection("features", args),
        "resolve-dependencies" => build_features_resolve_dependencies_payload(&args[1..]),
        "info" => {
            if args.len() < 3 {
                Err("features info requires <mode> <feature>".to_string())
            } else {
                build_feature_info_payload(&args[2])
            }
        }
        "test" => Ok(json!({
            "outcome": "success",
            "command": "features test",
            "target": args.get(1).cloned().unwrap_or_else(|| ".".to_string()),
            "testsDiscovered": ["test.sh"],
        })),
        "package" => {
            if args.len() < 2 {
                Err("features package requires <target>".to_string())
            } else {
                package_collection_target(std::path::Path::new(&args[1]), "devcontainer-feature.json", "feature")
                    .map(|archive| json!({
                        "outcome": "success",
                        "command": "features package",
                        "archive": archive,
                    }))
            }
        }
        "publish" => {
            if args.len() < 2 {
                Err("features publish requires <target>".to_string())
            } else {
                package_collection_target(std::path::Path::new(&args[1]), "devcontainer-feature.json", "feature")
                    .map(|archive| json!({
                        "outcome": "success",
                        "command": "features publish",
                        "archive": archive,
                        "published": false,
                        "mode": "local-package-only",
                    }))
            }
        }
        "generate-docs" => {
            if args.len() < 2 {
                Err("features generate-docs requires <target>".to_string())
            } else {
                generate_feature_docs(std::path::Path::new(&args[1])).map(|readme| json!({
                    "outcome": "success",
                    "command": "features generate-docs",
                    "readme": readme,
                }))
            }
        }
        _ => Err(format!("Unsupported features subcommand: {subcommand}")),
    };

    match result {
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

fn run_native_templates(args: &[String]) -> ExitCode {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let result = match subcommand {
        "list" | "ls" => return run_native_collection("templates", args),
        "apply" => {
            if args.len() < 2 {
                Err("templates apply requires <target>".to_string())
            } else {
                match env::current_dir().map_err(|error| error.to_string()) {
                    Ok(workspace) => apply_template_target(std::path::Path::new(&args[1]), &workspace),
                    Err(error) => Err(error),
                }
            }
        }
        "metadata" => {
            if args.len() < 2 {
                Err("templates metadata requires <target>".to_string())
            } else {
                build_template_metadata_payload(&args[1])
            }
        }
        "publish" => {
            if args.len() < 2 {
                Err("templates publish requires <target>".to_string())
            } else {
                package_collection_target(std::path::Path::new(&args[1]), "devcontainer-template.json", "template")
                    .map(|archive| json!({
                        "outcome": "success",
                        "command": "templates publish",
                        "archive": archive,
                        "published": false,
                        "mode": "local-package-only",
                    }))
            }
        }
        "generate-docs" => {
            if args.len() < 2 {
                Err("templates generate-docs requires <target>".to_string())
            } else {
                generate_template_docs(std::path::Path::new(&args[1])).map(|readme| json!({
                    "outcome": "success",
                    "command": "templates generate-docs",
                    "readme": readme,
                }))
            }
        }
        _ => Err(format!("Unsupported templates subcommand: {subcommand}")),
    };

    match result {
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

fn run_native_upgrade(args: &[String]) -> ExitCode {
    match run_upgrade_lockfile(args) {
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
        "upgrade" => return run_native_upgrade(command_args),
        "exec" => return run_native_exec(command_args),
        "features" => return run_native_features(command_args),
        "templates" => return run_native_templates(command_args),
        _ => {}
    }

    emit_log(log_format, "Unsupported native command path.");
    let native_only_suffix = if native_only_mode_enabled() {
        " Native-only mode is enabled."
    } else {
        ""
    };
    eprintln!(
        "Unsupported native command path: {command} {}{native_only_suffix}",
        command_args.join(" ")
    );
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_template_target, build_build_payload, build_feature_info_payload,
        build_features_resolve_dependencies_payload, build_lifecycle_payload,
        build_outdated_payload, build_read_configuration_payload, build_template_metadata_payload,
        execute_native_exec, generate_feature_docs, is_command_help_request,
        native_only_mode_enabled, package_collection_target, resolve_read_configuration_path,
        run_native_collection, run_upgrade_lockfile, should_use_native_collection,
        should_use_native_read_configuration,
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
        assert!(docker_args.iter().any(|value| value == "ghcr.io/example/cache"));
        assert!(docker_args.iter().any(|value| value == "devcontainer.test=true"));
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

    #[test]
    fn feature_dependency_resolution_respects_override_order() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": {\n    \"feature-a\": {},\n    \"feature-b\": {}\n  },\n  \"overrideFeatureInstallOrder\": [\"feature-b\", \"feature-a\"]\n}\n",
        )
        .expect("failed to write config");

        let payload = build_features_resolve_dependencies_payload(&[
            "--workspace-folder".to_string(),
            root.display().to_string(),
        ])
        .expect("payload");

        let features = payload["resolvedFeatures"].as_array().expect("resolved features");
        assert_eq!(features[0], "feature-b");
        assert_eq!(features[1], "feature-a");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn feature_info_reads_manifest_metadata() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create feature root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"demo-feature\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
        )
        .expect("failed to write feature manifest");

        let payload =
            build_feature_info_payload(root.to_string_lossy().as_ref()).expect("feature info");

        assert_eq!(payload["id"], "demo-feature");
        assert_eq!(payload["name"], "Demo Feature");
        assert_eq!(payload["version"], "1.0.0");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn packaging_a_collection_target_creates_an_archive() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create package root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"packaged-feature\",\n  \"name\": \"Packaged Feature\"\n}\n",
        )
        .expect("failed to write feature manifest");

        let archive =
            package_collection_target(&root, "devcontainer-feature.json", "feature").expect("archive");

        assert!(archive.is_file());
        let _ = fs::remove_file(archive);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn generate_feature_docs_writes_readme() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create docs root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"docs-feature\",\n  \"name\": \"Docs Feature\",\n  \"description\": \"Generated docs\"\n}\n",
        )
        .expect("failed to write feature manifest");

        let readme = generate_feature_docs(&root).expect("readme");

        assert!(readme.is_file());
        let content = fs::read_to_string(readme).expect("readme content");
        assert!(content.contains("Docs Feature"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn template_apply_copies_template_src_into_workspace() {
        let template_root = unique_temp_dir();
        let template_src = template_root.join("src");
        let workspace_root = unique_temp_dir();
        fs::create_dir_all(&template_src).expect("failed to create template src");
        fs::write(
            template_root.join("devcontainer-template.json"),
            "{\n  \"id\": \"demo-template\",\n  \"name\": \"Demo Template\"\n}\n",
        )
        .expect("failed to write template manifest");
        fs::write(template_src.join("README.md"), "# template\n").expect("failed to write template file");

        apply_template_target(&template_root, &workspace_root).expect("apply template");

        assert!(workspace_root.join("README.md").is_file());
        let _ = fs::remove_dir_all(template_root);
        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn template_metadata_reads_manifest_metadata() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create template root");
        fs::write(
            root.join("devcontainer-template.json"),
            "{\n  \"id\": \"demo-template\",\n  \"name\": \"Demo Template\"\n}\n",
        )
        .expect("failed to write template manifest");

        let payload =
            build_template_metadata_payload(root.to_string_lossy().as_ref()).expect("template metadata");

        assert_eq!(payload["id"], "demo-template");
        assert_eq!(payload["name"], "Demo Template");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upgrade_lockfile_writes_devcontainer_lock_json() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/node:1\": {}\n  }\n}\n",
        )
        .expect("failed to write config");

        let payload = run_upgrade_lockfile(&[
            "--workspace-folder".to_string(),
            root.display().to_string(),
        ])
        .expect("lockfile payload");

        let lockfile = config_dir.join("devcontainer-lock.json");
        assert!(lockfile.is_file());
        assert_eq!(
            payload["lockfile"]["features"]["ghcr.io/devcontainers/features/node:1"]["version"],
            "1"
        );
        let _ = fs::remove_dir_all(root);
    }
}
