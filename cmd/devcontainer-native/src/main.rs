use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

mod phase4;

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
