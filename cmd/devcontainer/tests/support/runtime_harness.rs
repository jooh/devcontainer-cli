#![allow(dead_code)]

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use super::test_support::{devcontainer_command, unique_temp_dir};

pub struct RuntimeHarness {
    pub root: PathBuf,
    pub log_dir: PathBuf,
    pub fake_podman: PathBuf,
}

impl RuntimeHarness {
    pub fn new() -> Self {
        let root = unique_temp_dir("devcontainer-runtime-smoke");
        let log_dir = root.join("logs");
        fs::create_dir_all(&log_dir).expect("log dir");
        let fake_podman = write_fake_podman(&root);

        Self {
            root,
            log_dir,
            fake_podman,
        }
    }

    pub fn workspace(&self) -> PathBuf {
        self.root.join("workspace")
    }

    pub fn run(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        self.run_in_dir(args, envs, None)
    }

    pub fn run_in_dir(&self, args: &[&str], envs: &[(&str, &str)], cwd: Option<&Path>) -> Output {
        let mut command = command(args, cwd);
        command.env(
            "FAKE_PODMAN_LOG_DIR",
            self.log_dir.to_string_lossy().as_ref(),
        );
        for (key, value) in envs {
            command.env(key, value);
        }
        command.output().expect("command should run")
    }

    pub fn run_with_input(&self, args: &[&str], envs: &[(&str, &str)], input: &str) -> Output {
        let mut command = command(args, None);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.env(
            "FAKE_PODMAN_LOG_DIR",
            self.log_dir.to_string_lossy().as_ref(),
        );
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

    pub fn read_invocations(&self) -> String {
        fs::read_to_string(self.log_dir.join("invocations.log")).expect("invocations")
    }

    pub fn read_exec_log(&self) -> String {
        fs::read_to_string(self.log_dir.join("exec.log")).expect("exec log")
    }

    pub fn read_exec_argv_log(&self) -> String {
        fs::read_to_string(self.log_dir.join("exec-argv.log")).expect("exec argv log")
    }

    pub fn parse_stdout_json(&self, output: &Output) -> Value {
        serde_json::from_slice(&output.stdout).expect("json payload")
    }
}

pub fn write_devcontainer_config(root: &Path, body: &str) -> PathBuf {
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("config dir");
    let config_path = config_dir.join("devcontainer.json");
    fs::write(&config_path, body).expect("config write");
    config_path
}

fn command(args: &[&str], cwd: Option<&Path>) -> std::process::Command {
    let mut command = devcontainer_command(cwd);
    command.args(args);
    command
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
  compose)
    COMPOSE_FILES=""
    while [ "$#" -gt 0 ]; do
      case "${1:-}" in
        --project-name)
          shift 2
          ;;
        -f)
          if [ -n "$COMPOSE_FILES" ]; then
            COMPOSE_FILES="$COMPOSE_FILES
${2:-}"
          else
            COMPOSE_FILES="${2:-}"
          fi
          shift 2
          ;;
        *)
          break
          ;;
      esac
    done
    SUBCOMMAND="${1:-}"
    shift || true
    case "$SUBCOMMAND" in
      build)
        exit 0
        ;;
      version)
        if [ "${1:-}" = "--short" ] && [ -n "${FAKE_PODMAN_COMPOSE_VERSION:-}" ]; then
          printf '%s\n' "${FAKE_PODMAN_COMPOSE_VERSION}"
          exit 0
        fi
        echo "${FAKE_PODMAN_COMPOSE_VERSION:-2.24.0}"
        exit 0
        ;;
      up)
        : > "$LOG_DIR/compose-up-called"
        compose_labels_file="$LOG_DIR/compose-last-run-labels"
        : > "$compose_labels_file"
        if [ -n "$COMPOSE_FILES" ]; then
          old_ifs="${IFS- }"
          IFS='
'
          for compose_file in $COMPOSE_FILES; do
            [ -f "$compose_file" ] || continue
            while IFS= read -r line; do
              case "$line" in
                *"- '"*"'"|*"- \""*"\""|*" - "*"="*)
                  label="$(printf '%s' "$line" | sed -E "s/^[[:space:]]*-[[:space:]]*//; s/^['\"]//; s/['\"]$//")"
                  case "$label" in
                    *=*) printf '%s\n' "$label" >> "$compose_labels_file" ;;
                  esac
                  ;;
              esac
            done < "$compose_file"
          done
          IFS="$old_ifs"
        fi
        : > "$LOG_DIR/compose-service-running"
        exit 0
        ;;
      ps)
        if [ -n "${FAKE_PODMAN_COMPOSE_PS_OUTPUT_BEFORE_UP:-}" ] || [ -n "${FAKE_PODMAN_COMPOSE_PS_OUTPUT_AFTER_UP:-}" ]; then
          if [ -f "$LOG_DIR/compose-up-called" ]; then
            printf '%s\n' "${FAKE_PODMAN_COMPOSE_PS_OUTPUT_AFTER_UP:-}"
          else
            printf '%s\n' "${FAKE_PODMAN_COMPOSE_PS_OUTPUT_BEFORE_UP:-}"
          fi
          exit 0
        fi
        if [ "${FAKE_PODMAN_COMPOSE_PS_REQUIRE_ALL:-0}" = "1" ]; then
          if [ ! -f "$LOG_DIR/compose-service-running" ]; then
            case " $* " in
              *" -a "*) ;;
              *) exit 0 ;;
            esac
          fi
        fi
        if [ -n "${FAKE_PODMAN_COMPOSE_PS_OUTPUT:-}" ]; then
          printf '%s\n' "${FAKE_PODMAN_COMPOSE_PS_OUTPUT}"
          exit 0
        fi
        if [ -f "$LOG_DIR/compose-service-running" ]; then
          echo "fake-compose-container-id"
          exit 0
        fi
        exit 0
        ;;
      rm)
        rm -f "$LOG_DIR/compose-service-running"
        exit 0
        ;;
      *)
        echo "unsupported fake podman compose command: $SUBCOMMAND" >&2
        exit 1
        ;;
    esac
    ;;
  build)
    printf 'DOCKER_BUILDKIT=%s\n' "${DOCKER_BUILDKIT:-}" >> "$LOG_DIR/build-env.log"
    build_file=""
    while [ "$#" -gt 0 ]; do
      case "${1:-}" in
        --file)
          build_file="${2:-}"
          shift 2
          ;;
        *)
          shift
          ;;
      esac
    done
    if [ -n "$build_file" ] && [ -f "$build_file" ]; then
      {
        printf '%s\n' "BEGIN"
        cat "$build_file"
        printf '%s\n' "END"
      } >> "$LOG_DIR/build-dockerfiles.log"
    fi
    exit 0
    ;;
  push)
    exit 0
    ;;
  run)
    if [ -n "${FAKE_PODMAN_REQUIRE_FILE_BEFORE_RUN:-}" ] && [ ! -f "${FAKE_PODMAN_REQUIRE_FILE_BEFORE_RUN}" ]; then
      echo "missing required file before run" >&2
      exit 91
    fi
    labels_file="$LOG_DIR/last-run-labels"
    mounts_file="$LOG_DIR/last-run-mounts"
    : > "$labels_file"
    : > "$mounts_file"
    while [ "$#" -gt 0 ]; do
      case "${1:-}" in
        --label)
          printf '%s\n' "${2:-}" >> "$labels_file"
          shift 2
          ;;
        --mount)
          printf '%s\n' "${2:-}" >> "$mounts_file"
          shift 2
          ;;
        *)
          shift
          ;;
      esac
    done
    if [ -n "${FAKE_PODMAN_RUN_CONTAINER_ID:-}" ]; then
      echo "${FAKE_PODMAN_RUN_CONTAINER_ID}"
      exit 0
    fi
    echo "fake-container-id"
    exit 0
    ;;
  ps)
    if [ "${FAKE_PODMAN_PS_REQUIRE_ALL:-0}" = "1" ]; then
      case " $* " in
        *" -a "*) ;;
        *) exit 0 ;;
      esac
    fi
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
    if [ -n "${FAKE_PODMAN_PS_OUTPUT:-}" ]; then
      printf '%s\n' "${FAKE_PODMAN_PS_OUTPUT}"
      exit 0
    fi
    if [ "${FAKE_PODMAN_PS_DISABLE_DEFAULT:-0}" = "1" ]; then
      exit 0
    fi
    echo "fake-container-id"
    exit 0
    ;;
  inspect)
    if [ -n "${FAKE_PODMAN_INSPECT_FILE:-}" ]; then
      cat "${FAKE_PODMAN_INSPECT_FILE}"
      exit 0
    fi
    json_escape() {
      printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
    }
    labels_json=""
    if [ -f "$LOG_DIR/last-run-labels" ]; then
      while IFS= read -r label; do
        [ -n "$label" ] || continue
        key="${label%%=*}"
        value="${label#*=}"
        escaped_key="$(json_escape "$key")"
        escaped_value="$(json_escape "$value")"
        if [ -n "$labels_json" ]; then
          labels_json="$labels_json,"
        fi
        labels_json="$labels_json\"$escaped_key\":\"$escaped_value\""
      done < "$LOG_DIR/last-run-labels"
    fi
    if [ -f "$LOG_DIR/compose-last-run-labels" ]; then
      while IFS= read -r label; do
        [ -n "$label" ] || continue
        key="${label%%=*}"
        value="${label#*=}"
        escaped_key="$(json_escape "$key")"
        escaped_value="$(json_escape "$value")"
        if [ -n "$labels_json" ]; then
          labels_json="$labels_json,"
        fi
        labels_json="$labels_json\"$escaped_key\":\"$escaped_value\""
      done < "$LOG_DIR/compose-last-run-labels"
    fi
    mounts_json=""
    if [ -f "$LOG_DIR/last-run-mounts" ]; then
      while IFS= read -r mount; do
        [ -n "$mount" ] || continue
        source=""
        destination=""
        old_ifs="${IFS- }"
        IFS=','
        set -- $mount
        IFS="$old_ifs"
        for component in "$@"; do
          case "$component" in
            source=*)
              source="${component#source=}"
              ;;
            target=*|destination=*|dst=*)
              destination="${component#*=}"
              ;;
          esac
        done
        escaped_source="$(json_escape "$source")"
        escaped_destination="$(json_escape "$destination")"
        if [ -n "$mounts_json" ]; then
          mounts_json="$mounts_json,"
        fi
        mounts_json="$mounts_json{\"Source\":\"$escaped_source\",\"Destination\":\"$escaped_destination\"}"
      done < "$LOG_DIR/last-run-mounts"
    fi
    printf '[{"Config":{"Labels":{%s}},"Mounts":[%s]}]' "$labels_json" "$mounts_json"
    exit 0
    ;;
  start)
    exit 0
    ;;
  rm)
    exit 0
    ;;
  exec)
    saw_interactive=0
    exec_workdir=""
    exec_env_args=""
    while [ "$#" -gt 0 ]; do
      case "${1:-}" in
        --workdir|-w)
          exec_workdir="${2:-}"
          shift 2
          ;;
        -e)
          if [ -n "$exec_env_args" ]; then
            exec_env_args="$exec_env_args
${2:-}"
          else
            exec_env_args="${2:-}"
          fi
          shift 2
          ;;
        --user|-u)
          shift 2
          ;;
        -i)
          saw_interactive=1
          shift
          ;;
        -t)
          shift
          ;;
        *)
          container_id="${1:-}"
          shift
          break
          ;;
      esac
    done
    if [ "${FAKE_PODMAN_REQUIRE_INTERACTIVE:-0}" = "1" ] && [ "$saw_interactive" -ne 1 ]; then
      echo "missing interactive exec flag" >&2
      exit 90
    fi
    printf '%s\n' "$*" >> "$LOG_DIR/exec.log"
    {
      printf '%s\n' "BEGIN"
      for arg in "$@"; do
        printf '[%s]\n' "$arg"
      done
      printf '%s\n' "END"
    } >> "$LOG_DIR/exec-argv.log"
    if [ -n "${FAKE_PODMAN_EXEC_EXIT_CODE:-}" ]; then
      exit "${FAKE_PODMAN_EXEC_EXIT_CODE}"
    fi
    if [ "${1:-}" = "/bin/echo" ]; then
      shift
      printf '%s\n' "$*"
    elif [ "${1:-}" = "/bin/cat" ]; then
      shift
      cat
    elif { [ "${1:-}" = "/bin/bash" ] || [ "${1:-}" = "/bin/sh" ]; } && [ "${2:-}" = "-lc" ]; then
      shell_program="${1:-}"
      command_text="${3:-}"
      host_cwd=""
      if [ -f "$LOG_DIR/last-run-mounts" ]; then
        while IFS= read -r mount; do
          [ -n "$mount" ] || continue
          source=""
          destination=""
          old_ifs="${IFS- }"
          IFS=','
          set -- $mount
          IFS="$old_ifs"
          for component in "$@"; do
            case "$component" in
              source=*|src=*)
                source="${component#*=}"
                ;;
              target=*|destination=*|dst=*)
                destination="${component#*=}"
                ;;
            esac
          done
          if [ -n "$destination" ] && [ -n "$source" ]; then
            if [ "$destination" = "$exec_workdir" ]; then
              host_cwd="$source"
            fi
            command_text="$(printf '%s' "$command_text" | sed "s|$destination|$source|g")"
          fi
        done < "$LOG_DIR/last-run-mounts"
      fi
      old_ifs="${IFS- }"
      IFS='
'
      set --
      for entry in $exec_env_args; do
        set -- "$@" "$entry"
      done
      IFS="$old_ifs"
      if [ -n "$host_cwd" ] && [ -d "$host_cwd" ]; then
        (
          cd "$host_cwd" || exit 1
          env "$@" "$shell_program" -lc "$command_text"
        )
        exit $?
      fi
      if [ -n "$host_cwd" ] && [ ! -d "$host_cwd" ]; then
        exit 0
      fi
      env "$@" "$shell_program" -lc "$command_text"
      exit $?
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
