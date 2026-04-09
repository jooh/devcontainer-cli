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

fn command_description(command: &str) -> &'static str {
    match command {
        "up" => "Create and run dev container",
        "set-up" => "Set up an existing container as a dev container",
        "build" => "Build a dev container image",
        "run-user-commands" => "Run user commands",
        "read-configuration" => "Read configuration",
        "outdated" => "Show current and available versions",
        "upgrade" => "Upgrade lockfile",
        "exec" => "Execute a command on a running dev container",
        "features" => "Features commands",
        "templates" => "Templates commands",
        _ => "Native devcontainer command",
    }
}

pub fn print_help() {
    println!("devcontainer (native Rust CLI)");
    println!("\nUsage:\n  devcontainer [--log-format text|json] <command> [args...]\n");
    println!("Supported top-level commands:");
    for command in SUPPORTED_TOP_LEVEL_COMMANDS {
        println!("  - {command}: {}", command_description(command));
    }
    println!("\nReference:");
    println!("  - docs/cli/command-reference.md (generated from pinned upstream metadata)");
    println!("  - docs/upstream/parity-inventory.md (measured native parity inventory)");
}

pub fn print_command_help(command: &str) {
    println!("{}", command_description(command));
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
            println!("  - test <project-folder>");
            println!("  - package <target>");
            println!("  - publish <target>");
            println!("  - generate-docs <target>");
            println!(
                "  - published OCI flows remain partial; see docs/upstream/parity-inventory.md"
            );
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
            println!(
                "  - published OCI flows remain partial; see docs/upstream/parity-inventory.md"
            );
        }
        "build" | "up" | "exec" => {
            println!("Usage:\n  devcontainer {command} [args...]");
            println!("\nCurrent state:");
            println!("  - native runtime path is available");
            println!("  - upstream feature integration and some option coverage remain partial");
        }
        "set-up" | "run-user-commands" | "outdated" | "upgrade" => {
            println!("Usage:\n  devcontainer {command} [args...]");
            println!("\nNative support:");
            println!("  - set-up / run-user-commands invoke lifecycle hooks in-container");
            println!(
                "  - outdated / upgrade are native, but still rely on fixture/manual catalog data"
            );
        }
        _ => {
            println!("Usage:\n  devcontainer {command} [args...]");
        }
    }
    println!("\nReference:");
    println!("  - docs/cli/command-reference.md");
    println!("  - docs/upstream/parity-inventory.md");
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
