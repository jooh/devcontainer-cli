#!/bin/bash
set -e

# Wrapper script to run commands inside the dev container

fail() {
    echo "Error: $*" >&2
    exit 1
}

require_command() {
    local command_name="$1"
    local install_hint="$2"

    if ! command -v "$command_name" >/dev/null 2>&1; then
        fail "$install_hint"
    fi
}

run_devcontainer() {
    local exit_code=0

    if devcontainer "$@"; then
        return 0
    else
        exit_code=$?
    fi

    fail "devcontainer $1 failed with exit code $exit_code"
}

usage() {
    cat <<EOF
Usage: devcontainer-context.sh [--reset] [--help] [command] [args...]

Run commands inside the dev container, starting it if necessary.
Supports both Docker Compose-based and regular dev containers.

Options:
  --reset    Tear down the dev container first, then bring it back up
  --help     Show this help message

Examples:
  devcontainer-context.sh                        # Start dev container (if not running)
  devcontainer-context.sh uv run pytest          # Run pytest inside container
  devcontainer-context.sh --reset                # Fresh restart of container
  devcontainer-context.sh --reset uv sync        # Fresh restart, then run command
EOF
}

RESET=false
COMPOSE_FILES=()
HOST_CONFIG_DIR="$HOME/.config"
HOST_GIT_CONFIG=""
CONTAINER_GIT_CONFIG="/tmp/devcontainer-host-gitconfig"
CONTAINER_CONFIG_DIR="/root/.config"
CONTAINER_LOCAL_DIR="/root/.local"
CONTAINER_SHELL="/bin/bash"
CONTAINER_WORKSPACE_FOLDER=""
COMPOSE_PROJECT_NAME_OVERRIDE=""
LOCAL_VOLUME_NAME=""
PODMAN_DETACH_KEYS="ctrl-]"

resolve_devcontainer_config() {
    local candidate

    for candidate in \
        "$WORKSPACE_FOLDER/.devcontainer/devcontainer.json" \
        "$WORKSPACE_FOLDER/.devcontainer.json"
    do
        if [[ -f "$candidate" ]]; then
            DEVCONTAINER_CONFIG="$candidate"
            return
        fi
    done

    fail "No dev container configuration found in $WORKSPACE_FOLDER"
}

normalize_devcontainer_config() {
    perl -MJSON::PP -0777 -e '
        my $path = shift @ARGV;
        open my $handle, q{<}, $path or die "Failed to read $path: $!\n";
        local $/;
        my $content = <$handle>;
        close $handle;
        my $parser = JSON::PP->new->relaxed;
        my $parsed = $parser->decode($content);
        print JSON::PP->new->canonical->encode($parsed);
    ' "$DEVCONTAINER_CONFIG"
}

resolve_compose_file_path() {
    local config_dir="$1"
    local compose_path="$2"

    case "$compose_path" in
        /*)
            printf '%s\n' "$compose_path"
            ;;
        *)
            printf '%s/%s\n' "$config_dir" "$compose_path"
            ;;
    esac
}

detect_devcontainer_type() {
    local config_dir
    local compose_file
    local compose_kind
    local normalized_config
    local workspace_folder

    config_dir="$(dirname "$DEVCONTAINER_CONFIG")"

    normalized_config="$(normalize_devcontainer_config)" || fail "Unable to parse $DEVCONTAINER_CONFIG"

    compose_kind="$(jq -r '
        if has("dockerComposeFile") then
            .dockerComposeFile | type
        else
            "null"
        end
    ' <<< "$normalized_config")"

    case "$compose_kind" in
        null)
            DEVCONTAINER_KIND="regular"
            ;;
        string)
            DEVCONTAINER_KIND="compose"
            COMPOSE_FILES=("$(jq -r '.dockerComposeFile' <<< "$normalized_config")")
            ;;
        array)
            DEVCONTAINER_KIND="compose"
            if ! jq -e '.dockerComposeFile | all(.[]?; type == "string")' >/dev/null <<< "$normalized_config"; then
                fail "dockerComposeFile must be a string or an array of strings"
            fi
            mapfile -t COMPOSE_FILES < <(jq -r '.dockerComposeFile[]' <<< "$normalized_config")
            ;;
        *)
            fail "dockerComposeFile must be a string or an array of strings"
            ;;
    esac

    workspace_folder="$(jq -r '.workspaceFolder // empty' <<< "$normalized_config")"
    if [[ -z "$workspace_folder" ]]; then
        fail "workspaceFolder must be set in $DEVCONTAINER_CONFIG"
    fi
    CONTAINER_WORKSPACE_FOLDER="$workspace_folder"

    if [[ "$DEVCONTAINER_KIND" == "compose" ]]; then
        if [[ ${#COMPOSE_FILES[@]} -eq 0 ]]; then
            fail "No compose files were found in $DEVCONTAINER_CONFIG"
        fi

        for compose_file in "${!COMPOSE_FILES[@]}"; do
            COMPOSE_FILES[compose_file]="$(resolve_compose_file_path "$config_dir" "${COMPOSE_FILES[compose_file]}")"
        done
    fi
}

validate_compose_files() {
    local compose_file

    for compose_file in "${COMPOSE_FILES[@]}"; do
        if [[ ! -f "$compose_file" ]]; then
            fail "Compose file '$compose_file' configured in $DEVCONTAINER_CONFIG was not found"
        fi
    done
}

trim_whitespace() {
    sed 's/^[[:space:]]*//; s/[[:space:]]*$//'
}

sanitize_project_name() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9_-'
}

resolve_compose_project_name() {
    local candidate=""
    local env_file
    local compose_file
    local line
    local index
    local sanitized

    if [[ -n "${COMPOSE_PROJECT_NAME:-}" ]]; then
        candidate="$COMPOSE_PROJECT_NAME"
    else
        env_file="$(dirname "${COMPOSE_FILES[0]}")/.env"
        if [[ -f "$env_file" ]]; then
            while IFS= read -r line; do
                case "$line" in
                    COMPOSE_PROJECT_NAME=*)
                        candidate="$(printf '%s' "${line#COMPOSE_PROJECT_NAME=}" | trim_whitespace)"
                        break
                        ;;
                esac
            done < "$env_file"
        fi

        if [[ -z "$candidate" ]]; then
            for ((index=${#COMPOSE_FILES[@]} - 1; index >= 0; index--)); do
                compose_file="${COMPOSE_FILES[index]}"
                while IFS= read -r line; do
                    if [[ "$line" == [[:space:]]* ]]; then
                        continue
                    fi
                    case "$line" in
                        name:*)
                            candidate="$(printf '%s' "${line#name:}" | trim_whitespace)"
                            break 2
                            ;;
                    esac
                done < "$compose_file"
            done
        fi
    fi

    sanitized="$(sanitize_project_name "$candidate")"
    if [[ -n "$sanitized" ]]; then
        COMPOSE_PROJECT_NAME_OVERRIDE="$sanitized"
    fi
}

resolve_host_git_config() {
    if [[ -f "$HOME/.gitconfig" ]]; then
        HOST_GIT_CONFIG="$HOME/.gitconfig"
    fi
}

ensure_host_config_dir() {
    mkdir -p "$HOST_CONFIG_DIR"
}

resolve_local_volume_name() {
    LOCAL_VOLUME_NAME="${PROJECT_NAME}_root_local"
}

ensure_local_volume() {
    if [[ -z "$LOCAL_VOLUME_NAME" ]]; then
        fail "Local volume name was not resolved"
    fi

    podman volume create \
        --ignore \
        --label "devcontainer.local_folder=$WORKSPACE_FOLDER" \
        --label "devcontainer.config_file=$DEVCONTAINER_CONFIG" \
        --label "devcontainer.volume_role=root_local" \
        "$LOCAL_VOLUME_NAME" >/dev/null
}

is_container_running() {
    podman ps \
        --quiet \
        --filter "label=devcontainer.local_folder=$WORKSPACE_FOLDER" \
        --filter "label=devcontainer.config_file=$DEVCONTAINER_CONFIG"
}

get_existing_container_id() {
    podman ps \
        --all \
        --quiet \
        --filter "label=devcontainer.local_folder=$WORKSPACE_FOLDER" \
        --filter "label=devcontainer.config_file=$DEVCONTAINER_CONFIG"
}

container_has_git_config_mount() {
    local container_id="$1"

    if [[ -z "$HOST_GIT_CONFIG" || -z "$container_id" ]]; then
        return 1
    fi

    podman inspect "$container_id" \
        --format '{{range .Mounts}}{{println .Destination}}{{end}}' | grep -Fxq "$CONTAINER_GIT_CONFIG"
}

container_has_host_config_mount() {
    local container_id="$1"

    if [[ -z "$container_id" ]]; then
        return 1
    fi

    podman inspect "$container_id" \
        --format '{{range .Mounts}}{{println .Type .Destination}}{{end}}' | grep -Fxq "bind $CONTAINER_CONFIG_DIR"
}

container_has_root_local_volume_mount() {
    local container_id="$1"

    if [[ -z "$container_id" ]]; then
        return 1
    fi

    podman inspect "$container_id" \
        --format '{{range .Mounts}}{{println .Type .Destination}}{{end}}' | grep -Fxq "volume $CONTAINER_LOCAL_DIR"
}

warn_if_git_config_mount_missing() {
    local container_id

    if [[ -z "$HOST_GIT_CONFIG" ]]; then
        return
    fi

    container_id="$(get_existing_container_id | head -n 1)"
    if [[ -n "$container_id" ]] && ! container_has_git_config_mount "$container_id"; then
        echo "Notice: existing dev container was created without the host git config mount. Re-run with --reset to recreate it with git config sharing enabled." >&2
    fi
}

warn_if_host_config_mount_missing() {
    local container_id

    container_id="$(get_existing_container_id | head -n 1)"
    if [[ -n "$container_id" ]] && ! container_has_host_config_mount "$container_id"; then
        echo "Notice: existing dev container was created without the host ~/.config bind mount. Re-run with --reset to recreate it with shared config enabled." >&2
    fi
}

warn_if_root_local_volume_mount_missing() {
    local container_id

    container_id="$(get_existing_container_id | head -n 1)"
    if [[ -n "$container_id" ]] && ! container_has_root_local_volume_mount "$container_id"; then
        echo "Notice: existing dev container was created without the persistent /root/.local volume. Re-run with --reset to recreate it with local state persistence enabled." >&2
    fi
}

get_running_container_id() {
    is_container_running | head -n 1
}

exec_in_devcontainer() {
    local container_id
    local exec_args=()

    container_id="$(get_running_container_id)"
    if [[ -z "$container_id" ]]; then
        fail "No running dev container found for $WORKSPACE_FOLDER"
    fi

    exec_args=(exec)
    if [[ -t 0 && -t 1 ]]; then
        exec_args+=(--interactive --tty --detach-keys "$PODMAN_DETACH_KEYS")
    elif [[ -t 0 ]]; then
        exec_args+=(--interactive)
    fi

    exec_args+=(--workdir "$CONTAINER_WORKSPACE_FOLDER")
    exec_args+=(-e "SHELL=$CONTAINER_SHELL")

    if [[ -n "$HOST_GIT_CONFIG" ]]; then
        exec_args+=(-e "GIT_CONFIG_GLOBAL=$CONTAINER_GIT_CONFIG")
    fi

    exec podman "${exec_args[@]}" "$container_id" "$@"
}

compose_reset() {
    local compose_args=()
    local compose_file
    local project_name

    echo "Tearing down dev container..."
    for compose_file in "${COMPOSE_FILES[@]}"; do
        compose_args+=( -f "$compose_file" )
    done

    project_name="${COMPOSE_PROJECT_NAME_OVERRIDE:-$PROJECT_NAME}"
    podman compose "${compose_args[@]}" --project-name "$project_name" down -v --remove-orphans
}

start_devcontainer() {
    local up_args=(up)

    up_args+=("${DEVCONTAINER_UP_ARGS[@]}")

    if [[ "$1" == "reset" && "$DEVCONTAINER_KIND" != "compose" ]]; then
        up_args+=(--remove-existing-container)
    fi

    echo "Starting dev container..."
    run_devcontainer "${up_args[@]}"
    run_devcontainer run-user-commands "${DEVCONTAINER_RUNTIME_ARGS[@]}"
}

# Parse flags
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)
            usage
            exit 0
            ;;
        --reset)
            RESET=true
            shift
            ;;
        -*)
            echo "Error: Unknown option '$1'" >&2
            echo "Use --help for usage information" >&2
            exit 1
            ;;
        *)
            break
            ;;
    esac
done

WORKSPACE_FOLDER="$(pwd -P)"
WORKSPACE_NAME="$(basename "$WORKSPACE_FOLDER")"
# Project name used by devcontainer CLI: <folder_name>_devcontainer
PROJECT_NAME="${WORKSPACE_NAME}_devcontainer"

# Check dependencies
require_command "devcontainer" "The Dev Container CLI ('devcontainer') is not installed or not in PATH. Install it from https://github.com/devcontainers/cli or with 'npm install -g @devcontainers/cli'."
require_command "jq" "jq is required to inspect the dev container configuration"
require_command "perl" "perl is required to parse JSONC dev container configurations"
require_command "podman" "podman is not installed or not in PATH"

resolve_devcontainer_config
detect_devcontainer_type
resolve_host_git_config
ensure_host_config_dir
resolve_local_volume_name
ensure_local_volume

DOCKER_PATH="$(command -v podman)"
DEVCONTAINER_RUNTIME_ARGS=(--docker-path "$DOCKER_PATH" --workspace-folder "$WORKSPACE_FOLDER" --config "$DEVCONTAINER_CONFIG")
DEVCONTAINER_RUNTIME_ARGS+=(--remote-env "SHELL=$CONTAINER_SHELL")

if [[ "$DEVCONTAINER_KIND" == "compose" ]]; then
    # Export UID for compose file interpolation (bash special variable, not exported by default)
    export UID
    validate_compose_files
    resolve_compose_project_name
    require_command "podman-compose" "podman-compose is required for compose-based dev containers"
    COMPOSE_PATH="$(command -v podman-compose)"
    DEVCONTAINER_RUNTIME_ARGS+=(--docker-compose-path "$COMPOSE_PATH")
fi

if [[ -n "$HOST_GIT_CONFIG" ]]; then
    DEVCONTAINER_RUNTIME_ARGS+=(--remote-env "GIT_CONFIG_GLOBAL=$CONTAINER_GIT_CONFIG")
fi

DEVCONTAINER_UP_ARGS=("${DEVCONTAINER_RUNTIME_ARGS[@]}")
DEVCONTAINER_UP_ARGS+=(--mount "type=bind,source=$HOST_CONFIG_DIR,target=$CONTAINER_CONFIG_DIR")
DEVCONTAINER_UP_ARGS+=(--mount "type=volume,source=$LOCAL_VOLUME_NAME,target=$CONTAINER_LOCAL_DIR")

if [[ -n "$HOST_GIT_CONFIG" ]]; then
    DEVCONTAINER_UP_ARGS+=(--mount "type=bind,source=$HOST_GIT_CONFIG,target=$CONTAINER_GIT_CONFIG")
fi

if ! $RESET; then
    warn_if_host_config_mount_missing
    warn_if_root_local_volume_mount_missing
    warn_if_git_config_mount_missing
fi

if $RESET; then
    if [[ "$DEVCONTAINER_KIND" == "compose" ]]; then
        compose_reset
    fi
    start_devcontainer reset
elif [[ -z "$(is_container_running)" ]]; then
    start_devcontainer start
fi

# If command provided, exec it
if [[ $# -gt 0 ]]; then
    exec_in_devcontainer "$@"
fi
