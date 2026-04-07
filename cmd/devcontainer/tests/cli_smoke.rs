use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir() -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let unique_id = NEXT_TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "devcontainer-cli-smoke-{}-{suffix}-{unique_id}",
        std::process::id()
    ))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

fn copy_recursive(source: &Path, destination: &Path) {
    let metadata = fs::metadata(source).expect("metadata");
    if metadata.is_dir() {
        fs::create_dir_all(destination).expect("create dir");
        for entry in fs::read_dir(source).expect("read dir") {
            let entry = entry.expect("dir entry");
            copy_recursive(&entry.path(), &destination.join(entry.file_name()));
        }
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::copy(source, destination).expect("copy file");
    }
}

fn run_read_configuration(
    args: &[&str],
    current_dir: Option<&Path>,
) -> (std::process::Output, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_devcontainer"));
    command.arg("read-configuration").args(args);
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }

    let output = command.output().expect("read-configuration should run");
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    let parsed = serde_json::from_str(&stdout).expect("json stdout");
    (output, parsed)
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
    assert!(stdout.contains("Create and run dev container"));
    assert!(stdout.contains("docs/cli/command-reference.md"));
}

#[test]
fn read_configuration_command_returns_configuration_payload() {
    let root = unique_temp_dir();
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("config dir");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\"\n}\n",
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
    let root = unique_temp_dir();
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

#[test]
fn outdated_supports_upstream_json_output_fixture() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-outdated-command");
    let workspace = unique_temp_dir();
    copy_recursive(&fixture, &workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .args([
            "outdated",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--output-format",
            "json",
        ])
        .output()
        .expect("outdated should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json payload");
    assert_eq!(
        payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["current"],
        "1.0.4"
    );
    assert_eq!(
        payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["wanted"],
        "1.0.5"
    );
    assert_eq!(
        payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["latest"],
        "1.2.0"
    );
    assert_eq!(
        payload["features"]["ghcr.io/codspace/versioning/foo:0.3.1"]["latest"],
        "2.11.1"
    );
    assert!(payload["features"]
        .as_object()
        .expect("features object")
        .contains_key("ghcr.io/codspace/doesnotexist:0.1.2"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn outdated_supports_text_output_fixture() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-outdated-command");
    let workspace = unique_temp_dir();
    copy_recursive(&fixture, &workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .args([
            "outdated",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--output-format",
            "text",
        ])
        .output()
        .expect("outdated should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Current"));
    assert!(stdout.contains("Wanted"));
    assert!(stdout.contains("Latest"));
    assert!(stdout.contains("ghcr.io/devcontainers/features/git"));
    assert!(stdout.contains("ghcr.io/devcontainers/features/azure-cli"));
    assert!(!stdout.contains("mylocalfeature"));
    assert!(!stdout.contains("terraform"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn upgrade_matches_upstream_lockfile_fixture() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-upgrade-command");
    let workspace = unique_temp_dir();
    copy_recursive(&fixture, &workspace);
    fs::copy(
        workspace.join("outdated.devcontainer-lock.json"),
        workspace.join(".devcontainer-lock.json"),
    )
    .expect("seed lockfile");

    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .args([
            "upgrade",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("upgrade should run");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(workspace.join(".devcontainer-lock.json")).expect("actual lockfile"),
        fs::read_to_string(workspace.join("upgraded.devcontainer-lock.json"))
            .expect("expected lockfile")
    );

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn upgrade_with_feature_updates_config_and_dry_run_lockfile() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-upgrade-feature");
    let workspace = unique_temp_dir();
    copy_recursive(&fixture, &workspace);
    fs::copy(
        workspace.join("input.devcontainer.json"),
        workspace.join(".devcontainer.json"),
    )
    .expect("seed config");

    let output = Command::new(env!("CARGO_BIN_EXE_devcontainer"))
        .args([
            "upgrade",
            "--dry-run",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--feature",
            "ghcr.io/codspace/versioning/foo",
            "--target-version",
            "2",
        ])
        .output()
        .expect("upgrade should run");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(workspace.join(".devcontainer.json")).expect("updated config"),
        fs::read_to_string(workspace.join("expected.devcontainer.json")).expect("expected config")
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("dry-run lockfile");
    assert_eq!(
        payload["features"]["ghcr.io/codspace/versioning/foo:2"]["version"],
        "2.11.1"
    );

    let _ = fs::remove_dir_all(workspace);
}
