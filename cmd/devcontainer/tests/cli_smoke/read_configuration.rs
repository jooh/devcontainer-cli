use std::fs;

use serde_json::Value;

use crate::support::test_support::{devcontainer_command, repo_root, unique_temp_dir};

fn run_read_configuration(
    args: &[&str],
    current_dir: Option<&std::path::Path>,
) -> (std::process::Output, Value) {
    let mut command = devcontainer_command(current_dir);
    command.arg("read-configuration").args(args);

    let output = command.output().expect("read-configuration should run");
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    let parsed = serde_json::from_str(&stdout).expect("json stdout");
    (output, parsed)
}

#[test]
fn top_level_help_lists_supported_commands() {
    let output = devcontainer_command(None)
        .arg("--help")
        .output()
        .expect("help command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("read-configuration"));
    assert!(stdout.contains("templates"));
    assert!(stdout.contains("Create and run dev container"));
    assert!(stdout.contains("docs/cli/command-reference.md"));
}

#[test]
fn read_configuration_command_returns_configuration_payload() {
    let root = unique_temp_dir("devcontainer-cli-smoke");
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("config dir");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\"\n}\n",
    )
    .expect("config write");

    let output = devcontainer_command(None)
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
    assert!(stdout.contains("\"workspace\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn read_configuration_supports_upstream_subfolder_config() {
    let root = repo_root();
    let workspace = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("configs")
        .join("dockerfile-without-features");
    let config = workspace
        .join(".devcontainer")
        .join("subfolder")
        .join("devcontainer.json");

    let (output, payload) = run_read_configuration(
        &[
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ],
        None,
    );

    assert!(output.status.success());
    assert_eq!(
        payload["configuration"]["remoteEnv"]["SUBFOLDER_CONFIG_REMOTE_ENV"],
        "true"
    );
    assert_eq!(
        payload["configuration"]["configFilePath"],
        config
            .canonicalize()
            .expect("canonical config")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(
        payload["workspace"]["workspaceFolder"],
        "/workspaces/dockerfile-without-features"
    );
}

#[test]
fn read_configuration_uses_current_directory_with_upstream_fixture() {
    let root = repo_root();
    let workspace = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("configs")
        .join("image");

    let (output, payload) = run_read_configuration(&[], Some(&workspace));

    assert!(output.status.success());
    assert_eq!(payload["configuration"]["image"], "ubuntu:latest");
    assert_eq!(
        payload["configuration"]["remoteEnv"]["CONTAINER_PATH"],
        "${containerEnv:PATH}"
    );
    let local_path = payload["configuration"]["remoteEnv"]["LOCAL_PATH"]
        .as_str()
        .expect("local path");
    let expected_local_path = std::env::var_os("PATH").expect("PATH env set");
    assert_eq!(local_path, expected_local_path.to_string_lossy().as_ref());
}

#[test]
fn read_configuration_applies_upstream_style_local_substitution_defaults() {
    let root = repo_root();
    let workspace = root
        .join("src")
        .join("test")
        .join("parity")
        .join("fixtures")
        .join("config")
        .join("upstream-style-substitution");
    let config = workspace.join("devcontainer.json");

    let (output, payload) = run_read_configuration(
        &[
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ],
        None,
    );

    assert!(output.status.success());
    assert_eq!(
        payload["configuration"]["containerEnv"]["WORKSPACE_BASENAME"],
        "upstream-style-substitution"
    );
    assert_eq!(
        payload["configuration"]["containerEnv"]["DEFAULTED_ENV"],
        "fallback-value"
    );
    assert_eq!(
        payload["configuration"]["containerEnv"]["DEFAULT_WITH_COLONS"],
        "fallback"
    );
    assert_eq!(
        payload["configuration"]["containerEnv"]["MISSING_WITHOUT_DEFAULT"],
        "before--after"
    );
}

#[test]
fn read_configuration_merged_output_uses_upstream_pluralized_fields() {
    let root = unique_temp_dir("devcontainer-cli-smoke");
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("config dir");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\",\n  \"postAttachCommand\": \"echo attached\",\n  \"remoteUser\": \"vscode\"\n}\n",
    )
    .expect("config write");

    let (_, payload) = run_read_configuration(
        &[
            "--workspace-folder",
            root.to_string_lossy().as_ref(),
            "--include-merged-configuration",
        ],
        None,
    );

    assert_eq!(
        payload["configuration"]["postAttachCommand"],
        "echo attached"
    );
    assert_eq!(
        payload["mergedConfiguration"]["postAttachCommands"]
            .as_array()
            .expect("post attach commands")
            .len(),
        1
    );
    assert!(payload["mergedConfiguration"]
        .get("postAttachCommand")
        .is_none());
    assert_eq!(payload["mergedConfiguration"]["remoteUser"], "vscode");

    let _ = fs::remove_dir_all(root);
}
