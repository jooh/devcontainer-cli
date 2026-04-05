use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    std::env::temp_dir().join(format!("devcontainer-cli-smoke-{suffix}"))
}

#[test]
fn top_level_help_lists_supported_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .arg("--help")
        .output()
        .expect("help command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("read-configuration"));
    assert!(stdout.contains("templates"));
}

#[test]
fn read_configuration_command_returns_configuration_payload() {
    let root = unique_temp_dir();
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("config dir");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\"\n}\n",
    )
    .expect("config write");

    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .args([
            "read-configuration",
            "--workspace-folder",
            root.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("read-configuration should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("\"configuration\""));
    assert!(stdout.contains("\"metadata\""));

    let _ = fs::remove_dir_all(root);
}
