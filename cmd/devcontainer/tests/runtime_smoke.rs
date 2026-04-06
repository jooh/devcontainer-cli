use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    required_labels="${FAKE_PODMAN_PS_REQUIRE_LABELS:-}"
    if [ -z "$required_labels" ] && [ -n "${FAKE_PODMAN_PS_REQUIRE_LABEL:-}" ]; then
      required_labels="${FAKE_PODMAN_PS_REQUIRE_LABEL}"
    fi
    if [ -n "$required_labels" ]; then
      original_args="$*"
      old_ifs="${IFS- }"
      IFS='
'
      for expected_label in $required_labels; do
        case " $original_args " in
          *" --filter label=$expected_label "*) ;;
          *)
            IFS="$old_ifs"
            exit 0
            ;;
        esac
      done
      IFS="$old_ifs"
    fi
    if [ "${FAKE_PODMAN_PS_WITH_HEADER:-0}" = "1" ]; then
      echo "CONTAINER ID   IMAGE   COMMAND   CREATED   STATUS   PORTS   NAMES"
    fi
    echo "fake-container-id"
    exit 0
    ;;
  exec)
    saw_interactive=0
    while [ "$#" -gt 0 ]; do
      case "${1:-}" in
        --workdir|-w)
          shift 2
          ;;
        -e|--user|-u)
          shift 2
          ;;
        -i)
          saw_interactive=1
          shift
          ;;
        -t)
          shift
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
    if [ "${FAKE_PODMAN_REQUIRE_INTERACTIVE:-0}" = "1" ] && [ "$saw_interactive" -ne 1 ]; then
      echo "missing interactive exec flag" >&2
      exit 90
    fi
    printf '%s\n' "$*" >> "$LOG_DIR/exec.log"
    if [ "${1:-}" = "/bin/echo" ]; then
      shift
      printf '%s\n' "$*"
    elif [ "${1:-}" = "/bin/cat" ]; then
      shift
      cat
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
    run_command_in_dir(args, envs, None)
}

fn run_command_in_dir(
    args: &[&str],
    envs: &[(&str, &str)],
    cwd: Option<&Path>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_devcontainer"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().expect("command should run")
}

fn run_command_with_input(
    args: &[&str],
    envs: &[(&str, &str)],
    input: &str,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_devcontainer"));
    command.args(args);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = command.spawn().expect("command should spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("command should complete")
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
fn run_user_commands_resolves_container_ids_from_headered_ps_output() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"postCreateCommand\": \"echo post-create\"\n}\n",
    );

    let output = run_command(
        &[
            "run-user-commands",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[
            ("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref()),
            ("FAKE_PODMAN_PS_WITH_HEADER", "1"),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = fs::read_to_string(log_dir.join("invocations.log")).expect("invocations");
    assert!(invocations.contains("ps -q "));
    let exec_log = fs::read_to_string(log_dir.join("exec.log")).expect("exec log");
    assert!(exec_log.contains("sh -lc echo post-create"));
}

#[test]
fn lifecycle_commands_run_as_the_configured_remote_user() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"remoteUser\": \"vscode\",\n  \"postCreateCommand\": \"echo ready\"\n}\n",
    );

    let output = run_command(
        &[
            "up",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref())],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = fs::read_to_string(log_dir.join("invocations.log")).expect("invocations");
    assert!(invocations.contains("exec --workdir /workspaces/workspace --user vscode"));
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

#[test]
fn exec_with_config_uses_the_config_workspace_for_lookup_and_workdir() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);
    let workspace = root.join("workspace");
    let caller_dir = root.join("caller");
    fs::create_dir_all(&workspace).expect("workspace dir");
    fs::create_dir_all(&caller_dir).expect("caller dir");
    let config_path = write_devcontainer_config(&workspace, "{\n  \"image\": \"alpine:3.20\"\n}\n");
    let expected_workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.clone());

    let output = run_command_in_dir(
        &[
            "exec",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--config",
            config_path.to_string_lossy().as_ref(),
            "/bin/echo",
            "hello-from-config",
        ],
        &[
            ("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref()),
            (
                "FAKE_PODMAN_PS_REQUIRE_LABEL",
                format!("devcontainer.local_folder={}", expected_workspace.display()).as_str(),
            ),
        ],
        Some(&caller_dir),
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "hello-from-config\n"
    );

    let invocations = fs::read_to_string(log_dir.join("invocations.log")).expect("invocations");
    assert!(invocations.contains(&format!(
        "ps -q --filter label=devcontainer.local_folder={}",
        expected_workspace.display()
    )));
    assert!(invocations.contains(
        "exec --workdir /workspaces/workspace fake-container-id /bin/echo hello-from-config"
    ));
}

#[test]
fn nested_config_exec_uses_workspace_root_and_config_label() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);
    let workspace = root.join("workspace");
    let caller_dir = root.join("caller");
    let nested_config_dir = workspace.join(".devcontainer").join("python");
    fs::create_dir_all(&nested_config_dir).expect("nested config dir");
    fs::create_dir_all(&caller_dir).expect("caller dir");
    let config_path = nested_config_dir.join("devcontainer.json");
    fs::write(&config_path, "{\n  \"image\": \"alpine:3.20\"\n}\n").expect("config write");
    let expected_workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.clone());
    let expected_config = config_path
        .canonicalize()
        .unwrap_or_else(|_| config_path.clone());

    let required_labels = format!(
        "devcontainer.local_folder={}\ndevcontainer.config_file={}",
        expected_workspace.display(),
        expected_config.display()
    );

    let output = run_command_in_dir(
        &[
            "exec",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--config",
            config_path.to_string_lossy().as_ref(),
            "/bin/echo",
            "hello-from-nested-config",
        ],
        &[
            ("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref()),
            ("FAKE_PODMAN_PS_REQUIRE_LABELS", required_labels.as_str()),
        ],
        Some(&caller_dir),
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "hello-from-nested-config\n"
    );

    let invocations = fs::read_to_string(log_dir.join("invocations.log")).expect("invocations");
    assert!(invocations.contains(&format!(
        "ps -q --filter label=devcontainer.local_folder={}",
        expected_workspace.display()
    )));
    assert!(invocations.contains(&format!(
        "--filter label=devcontainer.config_file={}",
        expected_config.display()
    )));
    assert!(invocations.contains(
        "exec --workdir /workspaces/workspace fake-container-id /bin/echo hello-from-nested-config"
    ));
}

#[test]
fn interactive_exec_attaches_stdin() {
    let root = unique_temp_dir();
    let log_dir = root.join("logs");
    fs::create_dir_all(&log_dir).expect("log dir");
    let fake_podman = write_fake_podman(&root);

    let output = run_command_with_input(
        &[
            "exec",
            "--docker-path",
            fake_podman.to_string_lossy().as_ref(),
            "--container-id",
            "fake-container-id",
            "--interactive",
            "/bin/cat",
        ],
        &[
            ("FAKE_PODMAN_LOG_DIR", log_dir.to_string_lossy().as_ref()),
            ("FAKE_PODMAN_REQUIRE_INTERACTIVE", "1"),
        ],
        "hello-from-stdin\n",
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "hello-from-stdin\n"
    );

    let invocations = fs::read_to_string(log_dir.join("invocations.log")).expect("invocations");
    assert!(invocations.contains("exec -i "));
}
