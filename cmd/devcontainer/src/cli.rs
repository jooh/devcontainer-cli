use crate::output::{self, LogFormat};

pub const SUPPORTED_TOP_LEVEL_COMMANDS: [&str; 10] = [
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

pub fn print_help() {
    println!("devcontainer (native Rust CLI)");
    println!("\nUsage:\n  devcontainer [--log-format text|json] <command> [args...]\n");
    println!("Supported top-level commands:");
    for command in SUPPORTED_TOP_LEVEL_COMMANDS {
        println!("  - {command}");
    }
}

pub fn print_command_help(command: &str) {
    match command {
        "read-configuration" => {
            println!(
                "Usage:\n  devcontainer read-configuration [--workspace-folder <path>] [--config <path>]"
            );
            println!("\nNative support:");
            println!("  - resolves .devcontainer/devcontainer.json or .devcontainer.json");
            println!("  - supports --workspace-folder and --config");
        }
        "features" => {
            println!(
                "Usage:\n  devcontainer features <list|ls|resolve-dependencies|info|test|package|publish|generate-docs>"
            );
            println!("\nNative support:");
            println!("  - list / ls");
            println!("  - resolve-dependencies");
            println!("  - info manifest <feature>");
            println!("  - package <target>");
            println!("  - publish <target>");
            println!("  - generate-docs <target>");
            println!("  - test is not yet implemented natively");
        }
        "templates" => {
            println!(
                "Usage:\n  devcontainer templates <list|ls|apply|metadata|publish|generate-docs>"
            );
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

pub fn parse_log_format(args: &[String]) -> (&str, usize) {
    if args.len() >= 3 && args[0] == "--log-format" {
        return (args[1].as_str(), 2);
    }
    ("text", 0)
}

pub fn emit_log(log_format: &str, message: &str) {
    let format = match log_format {
        "json" => LogFormat::Json,
        _ => LogFormat::Text,
    };
    println!("{}", output::render_log(format, "info", message));
}

pub fn is_command_help_request(args: &[String]) -> bool {
    matches!(
        args.first().map(String::as_str),
        Some("--help") | Some("-h")
    )
}

#[cfg(test)]
mod tests {
    use super::is_command_help_request;

    #[test]
    fn detects_subcommand_help_requests() {
        assert!(is_command_help_request(&["--help".to_string()]));
        assert!(is_command_help_request(&["-h".to_string()]));
        assert!(!is_command_help_request(&["list".to_string()]));
    }
}
