use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    std::env::temp_dir().join(format!("devcontainer-cli-smoke-{suffix}"))
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate parent")
        .parent()
        .expect("repository root")
        .to_path_buf()
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

#[test]
fn read_configuration_uses_current_directory_when_workspace_folder_is_omitted() {
    let workspace = repository_root()
        .join("upstream")
        .join("src")
        .join("test")
        .join("configs")
        .join("image");

    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .arg("read-configuration")
        .current_dir(&workspace)
        .output()
        .expect("read-configuration should run from current directory");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("\"image\":\"ubuntu:latest\""));
    assert!(stdout.contains("\"configFile\""));
}

#[test]
fn read_configuration_fails_when_current_directory_has_no_config() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("temp dir");

    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .arg("read-configuration")
        .current_dir(&root)
        .output()
        .expect("read-configuration should exit with an error");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("Unable to locate a dev container config"));

    let _ = fs::remove_dir_all(root);
}
