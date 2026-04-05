use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    std::env::temp_dir().join(format!("devcontainer-runtime-smoke-{suffix}"))
}

fn write_fake_podman(root: &Path) -> PathBuf {
    let script_path = root.join("podman");
    let script = r#"#!/bin/sh
set -eu

LOG_DIR="${FAKE_PODMAN_LOG_DIR:?missing log dir}"
COMMAND="$1"
shift
printf '%s %s\n' "$COMMAND" "$*" >> "$LOG_DIR/invocations.log"

case "$COMMAND" in
  build)
    exit 0
    ;;
  run)
    echo "fake-container-id"
    exit 0
    ;;
  ps)
    echo "fake-container-id"
    exit 0
    ;;
  exec)
    while [ "$#" -gt 0 ]; do
      case "${1:-}" in
        --workdir)
          shift 2
          ;;
        -e)
          shift 2
          ;;
        fake-container-id)
          shift
          break
          ;;
        *)
          break
          ;;
      esac
    done
    if [ "${1:-}" = "fake-container-id" ]; then
      shift
    fi
    printf '%s\n' "$*" >> "$LOG_DIR/exec.log"
    if [ "${1:-}" = "/bin/echo" ]; then
      shift
      printf '%s\n' "$*"
    fi
    exit 0
    ;;
  *)
    echo "unsupported fake podman command: $COMMAND" >&2
    exit 1
    ;;
esac
"#;
    fs::write(&script_path, script).expect("failed to write fake podman");
    let mut permissions = fs::metadata(&script_path)
        .expect("fake podman metadata")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
    }
    fs::set_permissions(&script_path, permissions).expect("fake podman permissions");
    script_path
}

fn write_devcontainer_config(root: &Path, body: &str) -> PathBuf {
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("config dir");
    let config_path = config_dir.join("devcontainer.json");
    fs::write(&config_path, body).expect("config write");
    config_path
}

fn run_command(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_devcontainer"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("command should run")
}

#[test]
fn build_invokes_podman_for_dockerfile_configs() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);
    let workspace = root.join("workspace");
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    write_devcontainer_config(
        &workspace,
        "{\n  \"build\": {\n    \"dockerfile\": \"Dockerfile\",\n    \"context\": \".devcontainer\"\n  }\n}\n",
    );

    let output = run_command(
        &[
            "build",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--image-name",
            "example/native-build:test",
        ],
        &[("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref())],
    );

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let payload: Value = serde_json::from_str(&stdout).expect("json payload");
    assert_eq!(payload["outcome"], "success");
    assert_eq!(payload["imageName"], "example/native-build:test");

    let invocations = fs::read_to_string(log_dir.join("invocations.log")).expect("invocations");
    assert!(invocations.contains("build "));
    assert!(invocations.contains("--tag example/native-build:test"));
    assert!(invocations.contains("--file"));
}

#[test]
fn up_starts_a_container_and_exec_runs_inside_it() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"workspaceFolder\": \"/workspace\",\n  \"postCreateCommand\": \"echo ready\"\n}\n",
    );

    let up_output = run_command(
        &[
            "up",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--include-configuration",
        ],
        &[("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref())],
    );

    assert!(up_output.status.success(), "{up_output:?}");
    let up_stdout = String::from_utf8(up_output.stdout).expect("utf8 stdout");
    let up_payload: Value = serde_json::from_str(&up_stdout).expect("json payload");
    assert_eq!(up_payload["containerId"], "fake-container-id");
    assert_eq!(up_payload["remoteWorkspaceFolder"], "/workspace");
    assert_eq!(up_payload["configuration"]["image"], "alpine:3.20");

    let exec_output = run_command(
        &[
            "exec",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "/bin/echo",
            "hello-from-container",
        ],
        &[("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref())],
    );

    assert!(exec_output.status.success(), "{exec_output:?}");
    assert_eq!(
        String::from_utf8(exec_output.stdout).expect("utf8 stdout"),
        "hello-from-container\n"
    );

    let invocations = fs::read_to_string(log_dir.join("invocations.log")).expect("invocations");
    assert!(invocations.contains("run "));
    assert!(invocations
        .contains("exec --workdir /workspace fake-container-id /bin/echo hello-from-container"));

    let exec_log = fs::read_to_string(log_dir.join("exec.log")).expect("exec log");
    assert!(exec_log.contains("sh -lc echo ready"));
}

#[test]
fn set_up_and_run_user_commands_target_existing_containers() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("workspace dir");
    let config_path = write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"postCreateCommand\": \"echo post-create\",\n  \"postAttachCommand\": \"echo post-attach\"\n}\n",
    );

    let set_up_output = run_command(
        &[
            "set-up",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--container-id",
            "fake-container-id",
            "--config",
            config_path.to_string_lossy().as_ref(),
            "--include-configuration",
        ],
        &[("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref())],
    );

    assert!(set_up_output.status.success(), "{set_up_output:?}");
    let payload: Value =
        serde_json::from_slice(&set_up_output.stdout).expect("json payload for set-up");
    assert_eq!(payload["containerId"], "fake-container-id");
    assert_eq!(payload["configuration"]["image"], "alpine:3.20");

    let run_user_commands_output = run_command(
        &[
            "run-user-commands",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref())],
    );

    assert!(
        run_user_commands_output.status.success(),
        "{run_user_commands_output:?}"
    );
    let exec_log = fs::read_to_string(log_dir.join("exec.log")).expect("exec log");
    assert!(exec_log.contains("sh -lc echo post-create"));
    assert!(exec_log.contains("sh -lc echo post-attach"));
}
