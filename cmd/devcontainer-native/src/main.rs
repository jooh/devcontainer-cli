use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

mod phase4;
mod phase5;

const PHASE3_COMMANDS: [&str; 6] = [
    "read-configuration",
    "build",
    "up",
    "exec",
    "features",
    "templates",
];

fn print_help() {
    println!("devcontainer-native (phase 3)");
    println!("\nUsage:\n  devcontainer-native [--log-format text|json] <command> [args...]\n");
    println!("Supported top-level commands (forwarded to Node bridge):");
    for command in PHASE3_COMMANDS {
        println!("  - {command}");
    }
}

fn parse_log_format(args: &[String]) -> (&str, usize) {
    if args.len() >= 3 && args[0] == "--log-format" {
        return (args[1].as_str(), 2);
    }
    ("text", 0)
}

fn emit_log(log_format: &str, message: &str) {
    match log_format {
        "json" => println!(
            "{{\"level\":\"info\",\"message\":\"{}\"}}",
            message.replace('"', "\\\"")
        ),
        _ => println!("{message}"),
    }
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

fn resolve_read_configuration_path(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let workspace_folder = parse_option_value(args, "--workspace-folder")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| "Unable to determine workspace folder".to_string())?;

    let config_path = if let Some(config) = parse_option_value(args, "--config") {
        let explicit = PathBuf::from(config);
        if explicit.is_absolute() {
            explicit
        } else {
            workspace_folder.join(explicit)
        }
    } else {
        let modern = workspace_folder.join(".devcontainer/devcontainer.json");
        let legacy = workspace_folder.join(".devcontainer.json");
        if modern.is_file() {
            modern
        } else {
            legacy
        }
    };

    if !config_path.is_file() {
        return Err(format!(
            "Unable to locate a dev container config at {}",
            config_path.display()
        ));
    }

    let resolved_workspace = fs::canonicalize(&workspace_folder).unwrap_or(workspace_folder);
    let resolved_config = fs::canonicalize(&config_path).unwrap_or(config_path);
    Ok((resolved_workspace, resolved_config))
}

fn run_native_read_configuration(args: &[String]) -> ExitCode {
    let (workspace_folder, config_file) = match resolve_read_configuration_path(args) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    println!(
        "{{\"configuration\":{{\"workspaceFolder\":\"{}\",\"configFile\":\"{}\"}},\"metadata\":{{\"format\":\"jsonc\",\"pathResolution\":\"native-rust\"}}}}",
        workspace_folder.display().to_string().replace('\\', "\\\\").replace('"', "\\\""),
        config_file.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
    );

    ExitCode::SUCCESS
}

fn should_use_native_read_configuration(args: &[String]) -> bool {
    const SUPPORTED_OPTIONS: [&str; 2] = ["--workspace-folder", "--config"];
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with("--") {
            return false;
        }
        if !SUPPORTED_OPTIONS.contains(&arg.as_str()) {
            return false;
        }
        index += 2;
    }
    true
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

    if !PHASE3_COMMANDS.contains(&command.as_str()) {
        eprintln!("Unsupported command: {command}");
        return ExitCode::from(2);
    }

    let command_args = &raw_args[offset + 1..];
    match command.as_str() {
        "read-configuration" if should_use_native_read_configuration(command_args) => {
            return run_native_read_configuration(command_args);
        }
        "features" | "templates" if should_use_native_collection(command_args) => {
            return run_native_collection(command, command_args);
        }
        _ => {}
    }

    emit_log(log_format, "Delegating command to Node compatibility bridge.");

    let bridge_script = match resolve_bridge_script() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Failed to resolve Node compatibility bridge path: {error}");
            return ExitCode::from(1);
        }
    };

    let status = Command::new("node")
        .arg(bridge_script)
        .args(&raw_args)
        .status();

    match status {
        Ok(exit_status) => match exit_status.code() {
            Some(code) => ExitCode::from(code as u8),
            None => ExitCode::from(1),
        },
        Err(error) => {
            eprintln!("Failed to invoke Node compatibility bridge: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_read_configuration_path, run_native_collection, should_use_native_collection,
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
        std::env::temp_dir().join(format!("devcontainer-native-test-{suffix}"))
    }

    #[test]
    fn resolves_modern_config_path_from_workspace_folder() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        let config = config_dir.join("devcontainer.json");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
        ];
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
    fn read_configuration_with_additional_flags_falls_back_to_node() {
        assert!(!should_use_native_read_configuration(&[
            "--workspace-folder".to_string(),
            "/workspace".to_string(),
            "--include-merged-configuration".to_string(),
        ]));
    }
}
