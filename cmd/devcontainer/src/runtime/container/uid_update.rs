//! UID-update image preparation and image-inspection helpers for native runtime flows.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::commands::common;

use super::super::compose;
use super::super::context::ResolvedConfig;
use super::super::engine;
use super::super::paths::unique_temp_path;

const UID_UPDATE_IMAGE_INSPECT_FORMAT: &str =
    "{{.Config.User}}\n{{.Os}}/{{.Architecture}}{{if .Variant}}/{{.Variant}}{{end}}";

#[derive(Debug, Eq, PartialEq)]
struct UidUpdateDetails {
    remote_user: String,
    image_user: String,
    updated_image_name: String,
    platform: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
struct ImageInspectDetails {
    user: String,
    platform: Option<String>,
}

pub(crate) fn prepare_up_image(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
) -> Result<String, String> {
    prepare_up_image_for_platform(
        resolved,
        args,
        image_name,
        is_uid_update_platform_supported(),
    )
}

fn prepare_up_image_for_platform(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    platform_supported: bool,
) -> Result<String, String> {
    if !should_update_remote_user_uid(&resolved.configuration, args, platform_supported) {
        return Ok(image_name.to_string());
    }

    let compose_service_user = if compose::uses_compose_config(&resolved.configuration) {
        compose::load_compose_spec(resolved)?.and_then(|spec| spec.user)
    } else {
        None
    };
    let Some(update) = resolve_uid_update_details(
        &resolved.configuration,
        args,
        &resolved.workspace_folder,
        image_name,
        compose_service_user.as_deref(),
    )?
    else {
        return Ok(image_name.to_string());
    };
    let (new_uid, new_gid) = host_uid_gid()?;
    let build_context = unique_uid_update_build_context();
    fs::create_dir_all(&build_context).map_err(|error| error.to_string())?;
    let dockerfile = uid_update_dockerfile_path();
    let mut build_args = vec!["build".to_string()];
    if let Some(platform) = &update.platform {
        build_args.push("--platform".to_string());
        build_args.push(platform.clone());
    }
    build_args.extend([
        "--build-arg".to_string(),
        format!("BASE_IMAGE={}", uid_update_base_image(args, image_name)),
        "--build-arg".to_string(),
        format!("REMOTE_USER={}", update.remote_user),
        "--build-arg".to_string(),
        format!("NEW_UID={new_uid}"),
        "--build-arg".to_string(),
        format!("NEW_GID={new_gid}"),
        "--build-arg".to_string(),
        format!("IMAGE_USER={}", update.image_user),
        "-t".to_string(),
        update.updated_image_name.clone(),
        "-f".to_string(),
        dockerfile.display().to_string(),
        build_context.display().to_string(),
    ]);

    let result = engine::run_engine(args, std::mem::take(&mut build_args))?;
    let _ = fs::remove_dir_all(&build_context);
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    Ok(update.updated_image_name)
}

pub(crate) fn should_update_remote_user_uid(
    configuration: &Value,
    args: &[String],
    platform_supported: bool,
) -> bool {
    if !platform_supported {
        return false;
    }

    let default_value = common::runtime_options(args)
        .update_remote_user_uid_default
        .unwrap_or_else(|| "on".to_string());
    if default_value == "never" {
        return false;
    }

    let should_update = configuration
        .get("updateRemoteUserUID")
        .and_then(Value::as_bool)
        .unwrap_or(default_value == "on");
    if !should_update {
        return false;
    }

    configuration.is_object()
}

fn uid_update_details(
    configuration: &Value,
    workspace_folder: &Path,
    image_name: &str,
    image_user: &str,
    runtime_user: Option<&str>,
    platform: Option<&str>,
) -> Option<UidUpdateDetails> {
    let remote_user = uid_update_remote_user(configuration, runtime_user, image_user)?;
    Some(UidUpdateDetails {
        remote_user,
        image_user: image_user.to_string(),
        updated_image_name: uid_update_image_name(workspace_folder, image_name),
        platform: platform.map(str::to_string),
    })
}

fn resolve_uid_update_details(
    configuration: &Value,
    args: &[String],
    workspace_folder: &Path,
    image_name: &str,
    compose_service_user: Option<&str>,
) -> Result<Option<UidUpdateDetails>, String> {
    let runtime_user = uid_update_run_args_user(configuration)
        .or_else(|| compose_service_user.map(str::to_string));
    if let Some(user) = uid_update_configured_user(configuration, runtime_user.as_deref()) {
        if !is_updatable_user(&user) {
            return Ok(None);
        }
    }

    let Some(image_details) =
        inspect_image_details_for_uid_update(args, configuration, image_name)?
    else {
        return Ok(None);
    };

    Ok(uid_update_details(
        configuration,
        workspace_folder,
        image_name,
        &image_details.user,
        runtime_user.as_deref(),
        image_details.platform.as_deref(),
    ))
}

fn uid_update_remote_user(
    configuration: &Value,
    run_args_user: Option<&str>,
    image_user: &str,
) -> Option<String> {
    let user = configuration
        .get("remoteUser")
        .or_else(|| configuration.get("containerUser"))
        .and_then(Value::as_str)
        .or(run_args_user)
        .unwrap_or(image_user);
    is_updatable_user(user).then(|| user.to_string())
}

fn uid_update_configured_user(
    configuration: &Value,
    run_args_user: Option<&str>,
) -> Option<String> {
    configuration
        .get("remoteUser")
        .or_else(|| configuration.get("containerUser"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| run_args_user.map(str::to_string))
}

fn uid_update_run_args_user(configuration: &Value) -> Option<String> {
    let run_args = configuration.get("runArgs").and_then(Value::as_array)?;
    for index in (0..run_args.len()).rev() {
        let Some(arg) = run_args[index].as_str() else {
            continue;
        };
        if matches!(arg, "-u" | "--user") {
            if let Some(user) = run_args.get(index + 1).and_then(Value::as_str) {
                return Some(user.to_string());
            }
            continue;
        }
        if let Some(user) = arg
            .strip_prefix("-u=")
            .or_else(|| arg.strip_prefix("--user="))
        {
            return Some(user.to_string());
        }
    }
    None
}

fn is_updatable_user(user: &str) -> bool {
    user != "root" && !user.chars().all(|character| character.is_ascii_digit())
}

fn inspect_image_details_for_uid_update(
    args: &[String],
    configuration: &Value,
    image_name: &str,
) -> Result<Option<ImageInspectDetails>, String> {
    match inspect_image_details_for_uid_update_once(args, image_name)? {
        Some(details) => Ok(Some(details)),
        None if configuration.get("image").and_then(Value::as_str).is_some() => {
            pull_image_for_uid_update(args, image_name)?;
            inspect_image_details_for_uid_update_once(args, image_name)
        }
        None => Ok(None),
    }
}

fn inspect_image_details_for_uid_update_once(
    args: &[String],
    image_name: &str,
) -> Result<Option<ImageInspectDetails>, String> {
    let result = engine::run_engine(
        args,
        vec![
            "image".to_string(),
            "inspect".to_string(),
            "--format".to_string(),
            UID_UPDATE_IMAGE_INSPECT_FORMAT.to_string(),
            image_name.to_string(),
        ],
    )?;
    if result.status_code != 0 {
        let error = engine::stderr_or_stdout(&result);
        if is_missing_local_image_inspect_error(&error) {
            return Ok(None);
        }
        return Err(error);
    }

    let mut lines = result.stdout.lines();
    let user = lines
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("root")
        .to_string();
    let platform = lines
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Ok(Some(ImageInspectDetails { user, platform }))
}

fn pull_image_for_uid_update(args: &[String], image_name: &str) -> Result<(), String> {
    let result = engine::run_engine(args, vec!["pull".to_string(), image_name.to_string()])?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

fn is_missing_local_image_inspect_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("no such image") || error.contains("image not known")
}

fn uid_update_image_name(workspace_folder: &Path, image_name: &str) -> String {
    let local_image_name = uid_update_local_image_name(workspace_folder);
    let base_image_name = if image_name.starts_with(&local_image_name) {
        image_name
    } else {
        local_image_name.as_str()
    };
    format!("{base_image_name}-uid")
}

fn uid_update_local_image_name(workspace_folder: &Path) -> String {
    let basename = workspace_folder
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace")
        .chars()
        .flat_map(|character| character.to_lowercase())
        .map(|character| {
            if character.is_ascii_lowercase() || character.is_ascii_digit() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let hash = Sha256::digest(workspace_folder.to_string_lossy().as_bytes());
    format!("vsc-{basename}-{hash:x}")
}

fn uid_update_base_image(args: &[String], image_name: &str) -> String {
    if uses_podman_engine(args) && !has_registry_hostname(image_name) {
        return format!("localhost/{image_name}");
    }
    image_name.to_string()
}

fn uses_podman_engine(args: &[String]) -> bool {
    common::parse_option_value(args, "--docker-path")
        .and_then(|value| {
            Path::new(&value)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .is_some_and(|value| value.eq_ignore_ascii_case("podman"))
}

fn has_registry_hostname(image_name: &str) -> bool {
    if image_name.starts_with("localhost/") {
        return true;
    }
    let dot = image_name.find('.');
    let slash = image_name.find('/');
    dot.is_some_and(|dot| slash.is_some_and(|slash| dot < slash))
}

fn is_uid_update_platform_supported() -> bool {
    cfg!(target_os = "linux")
}

fn uid_update_dockerfile_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("upstream")
        .join("scripts")
        .join("updateUID.Dockerfile")
}

fn unique_uid_update_build_context() -> PathBuf {
    unique_temp_path("devcontainer-update-uid", None)
}

fn host_uid_gid() -> Result<(String, String), String> {
    let uid = command_stdout("id", &["-u"])?;
    let gid = command_stdout("id", &["-g"])?;
    Ok((uid, gid))
}

fn command_stdout(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    //! Unit tests for UID-update image preparation helpers.

    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use crate::runtime::context::ResolvedConfig;

    use super::{
        prepare_up_image_for_platform, should_update_remote_user_uid, uid_update_details,
        uid_update_local_image_name, unique_uid_update_build_context,
    };

    #[test]
    fn remote_user_uid_update_defaults_to_on_for_supported_platforms() {
        assert!(should_update_remote_user_uid(
            &json!({
                "remoteUser": "vscode"
            }),
            &[],
            true,
        ));
    }

    #[test]
    fn remote_user_uid_update_can_inspect_image_user_when_config_omits_users() {
        assert!(should_update_remote_user_uid(&json!({}), &[], true));
    }

    #[test]
    fn remote_user_uid_update_respects_option_and_config_overrides() {
        assert!(!should_update_remote_user_uid(
            &json!({
                "remoteUser": "vscode"
            }),
            &[
                "--update-remote-user-uid-default".to_string(),
                "off".to_string(),
            ],
            true,
        ));
        assert!(!should_update_remote_user_uid(
            &json!({
                "remoteUser": "vscode",
                "updateRemoteUserUID": false
            }),
            &[],
            true,
        ));
        assert!(!should_update_remote_user_uid(
            &json!({
                "remoteUser": "vscode"
            }),
            &[],
            false,
        ));
    }

    #[test]
    fn uid_update_details_fall_back_to_the_image_user() {
        let details = uid_update_details(
            &json!({}),
            Path::new("/tmp/example-workspace"),
            "ghcr.io/example/app:latest",
            "node",
            None,
            Some("linux/amd64"),
        )
        .expect("uid update details");

        assert_eq!(details.remote_user, "node");
        assert_eq!(details.image_user, "node");
        assert_eq!(details.platform.as_deref(), Some("linux/amd64"));
        assert!(details
            .updated_image_name
            .starts_with("vsc-example-workspace-"));
        assert!(details.updated_image_name.ends_with("-uid"));
    }

    #[test]
    fn uid_update_details_preserve_the_image_user_when_remote_user_is_overridden() {
        let details = uid_update_details(
            &json!({
                "remoteUser": "vscode"
            }),
            Path::new("/tmp/example-workspace"),
            "ghcr.io/example/app:latest",
            "node",
            None,
            None,
        )
        .expect("uid update details");

        assert_eq!(details.remote_user, "vscode");
        assert_eq!(details.image_user, "node");
    }

    #[test]
    fn uid_update_details_use_a_local_tag_for_digest_pinned_images() {
        let details = uid_update_details(
            &json!({
                "remoteUser": "vscode"
            }),
            Path::new("/tmp/example-workspace"),
            "ghcr.io/example/app@sha256:0123456789abcdef",
            "node",
            None,
            None,
        )
        .expect("uid update details");

        assert!(details
            .updated_image_name
            .starts_with("vsc-example-workspace-"));
        assert!(details.updated_image_name.ends_with("-uid"));
        assert!(!details.updated_image_name.contains('@'));
    }

    #[test]
    fn prepare_up_image_pulls_missing_image_before_uid_update() {
        let fixture = FakeEngineFixture::new();
        fixture.write("image-inspect.exit", "1\n");
        fixture.write(
            "image-inspect.stderr",
            "Error: No such image: ghcr.io/example/app:latest\n",
        );
        fixture.write(
            "image-inspect-after-pull.stdout",
            &image_inspect_output("root", Some("linux/amd64")),
        );

        let workspace = fixture.root.join("workspace");
        fs::create_dir_all(&workspace).expect("workspace dir");
        let resolved = resolved_config(
            json!({
                "image": "ghcr.io/example/app:latest",
                "remoteUser": "vscode",
            }),
            &workspace,
        );

        let updated_image = prepare_up_image_for_platform(
            &resolved,
            &fixture.args(),
            "ghcr.io/example/app:latest",
            true,
        )
        .expect("prepare up image");

        assert_ne!(updated_image, "ghcr.io/example/app:latest");
        assert!(updated_image.ends_with("-uid"));
        let invocations = fixture.invocations();
        assert!(invocations.contains(&format!(
            "image inspect --format {} ghcr.io/example/app:latest",
            super::UID_UPDATE_IMAGE_INSPECT_FORMAT
        )));
        assert!(invocations.contains("pull ghcr.io/example/app:latest"));
        assert!(invocations.contains("build "));
        assert!(invocations.contains("--build-arg REMOTE_USER=vscode"));
        assert!(invocations.contains("--build-arg IMAGE_USER=root"));
    }

    #[test]
    fn prepare_up_image_uses_run_args_user_for_uid_update_selection() {
        let fixture = FakeEngineFixture::new();
        fixture.write(
            "image-inspect.stdout",
            &image_inspect_output("root", Some("linux/amd64")),
        );

        let workspace = fixture.root.join("workspace");
        fs::create_dir_all(&workspace).expect("workspace dir");
        let resolved = resolved_config(
            json!({
                "runArgs": ["--user", "vscode"]
            }),
            &workspace,
        );

        let updated_image =
            prepare_up_image_for_platform(&resolved, &fixture.args(), "alpine:3.20", true)
                .expect("prepare up image");

        assert_ne!(updated_image, "alpine:3.20");
        assert!(updated_image.ends_with("-uid"));
        let invocations = fixture.invocations();
        assert!(invocations.contains("build "));
        assert!(invocations.contains("--build-arg REMOTE_USER=vscode"));
        assert!(invocations.contains("--build-arg IMAGE_USER=root"));
    }

    #[test]
    fn prepare_up_image_preserves_the_inspected_platform_for_uid_update_builds() {
        let fixture = FakeEngineFixture::new();
        fixture.write(
            "image-inspect.stdout",
            &image_inspect_output("node", Some("linux/arm64/v8")),
        );

        let workspace = fixture.root.join("workspace");
        fs::create_dir_all(&workspace).expect("workspace dir");
        let resolved = resolved_config(
            json!({
                "remoteUser": "vscode"
            }),
            &workspace,
        );

        let updated_image = prepare_up_image_for_platform(
            &resolved,
            &fixture.args(),
            "ghcr.io/example/app:latest",
            true,
        )
        .expect("prepare up image");

        assert!(updated_image.ends_with("-uid"));
        let invocations = fixture.invocations();
        assert!(invocations.contains("build "));
        assert!(invocations.contains("--platform linux/arm64/v8"));
    }

    #[test]
    fn prepare_up_image_prefixes_local_podman_base_images_with_localhost() {
        let fixture = FakeEngineFixture::new();
        fixture.write(
            "image-inspect.stdout",
            &image_inspect_output("node", Some("linux/amd64")),
        );

        let workspace = fixture.root.join("workspace");
        fs::create_dir_all(&workspace).expect("workspace dir");
        let resolved = resolved_config(
            json!({
                "remoteUser": "vscode"
            }),
            &workspace,
        );

        let local_image_name = uid_update_local_image_name(&workspace);
        let updated_image = prepare_up_image_for_platform(
            &resolved,
            &fixture.args_with_podman_name(),
            &local_image_name,
            true,
        )
        .expect("prepare up image");

        assert_eq!(updated_image, format!("{local_image_name}-uid"));
        let invocations = fixture.invocations();
        assert!(invocations.contains("build "));
        assert!(invocations.contains(&format!(
            "--build-arg BASE_IMAGE=localhost/{local_image_name}"
        )));
    }

    #[test]
    fn prepare_up_image_uses_compose_service_user_for_uid_update_selection() {
        let fixture = FakeEngineFixture::new();
        fixture.write(
            "image-inspect.stdout",
            &image_inspect_output("root", Some("linux/amd64")),
        );

        let workspace = fixture.root.join("workspace");
        let config_root = workspace.join(".devcontainer");
        fs::create_dir_all(&config_root).expect("config dir");
        fs::write(
            config_root.join("docker-compose.yml"),
            "services:\n  app:\n    image: ghcr.io/example/app:latest\n    user: vscode\n",
        )
        .expect("compose file");
        let resolved = ResolvedConfig {
            workspace_folder: workspace.clone(),
            config_file: config_root.join("devcontainer.json"),
            configuration: json!({
                "dockerComposeFile": "docker-compose.yml",
                "service": "app",
            }),
        };

        let updated_image = prepare_up_image_for_platform(
            &resolved,
            &fixture.args(),
            "ghcr.io/example/app:latest",
            true,
        )
        .expect("prepare up image");

        assert!(updated_image.ends_with("-uid"));
        let invocations = fixture.invocations();
        assert!(invocations.contains("build "));
        assert!(invocations.contains("--build-arg REMOTE_USER=vscode"));
    }

    fn resolved_config(
        configuration: serde_json::Value,
        workspace_folder: &Path,
    ) -> ResolvedConfig {
        ResolvedConfig {
            workspace_folder: workspace_folder.to_path_buf(),
            config_file: workspace_folder
                .join(".devcontainer")
                .join("devcontainer.json"),
            configuration,
        }
    }

    struct FakeEngineFixture {
        root: PathBuf,
        engine_path: PathBuf,
        podman_engine_path: PathBuf,
        invocation_log: PathBuf,
    }

    impl FakeEngineFixture {
        fn new() -> Self {
            let root = unique_uid_update_build_context();
            fs::create_dir_all(&root).expect("fixture root");
            let engine_path = root.join("fake-engine");
            let podman_engine_path = root.join("podman");
            let invocation_log = root.join("invocations.log");
            let script = r#"#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
LOG="$ROOT/invocations.log"
printf '%s %s\n' "$1" "$*" >> "$LOG"

COMMAND="$1"
shift || true
case "$COMMAND" in
  image)
    SUBCOMMAND="${1:-}"
    shift || true
            case "$SUBCOMMAND" in
              inspect)
                if [ -f "$ROOT/pulled" ]; then
                  if [ -f "$ROOT/image-inspect-after-pull.stdout" ]; then
                    cat "$ROOT/image-inspect-after-pull.stdout"
                  fi
                  if [ -f "$ROOT/image-inspect-after-pull.stderr" ]; then
                    cat "$ROOT/image-inspect-after-pull.stderr" >&2
                  fi
                  if [ -f "$ROOT/image-inspect-after-pull.exit" ]; then
                    exit "$(tr -d '\n' < "$ROOT/image-inspect-after-pull.exit")"
                  fi
                  exit 0
                fi
                if [ -f "$ROOT/image-inspect.stdout" ]; then
                  cat "$ROOT/image-inspect.stdout"
                fi
                if [ -f "$ROOT/image-inspect.stderr" ]; then
                  cat "$ROOT/image-inspect.stderr" >&2
        fi
        if [ -f "$ROOT/image-inspect.exit" ]; then
          exit "$(tr -d '\n' < "$ROOT/image-inspect.exit")"
        fi
        exit 0
        ;;
    esac
    ;;
  pull)
    touch "$ROOT/pulled"
    if [ -f "$ROOT/pull.stdout" ]; then
      cat "$ROOT/pull.stdout"
    fi
    if [ -f "$ROOT/pull.stderr" ]; then
      cat "$ROOT/pull.stderr" >&2
    fi
    if [ -f "$ROOT/pull.exit" ]; then
      exit "$(tr -d '\n' < "$ROOT/pull.exit")"
    fi
    exit 0
    ;;
  build)
    if [ -f "$ROOT/build.stdout" ]; then
      cat "$ROOT/build.stdout"
    fi
    if [ -f "$ROOT/build.stderr" ]; then
      cat "$ROOT/build.stderr" >&2
    fi
    if [ -f "$ROOT/build.exit" ]; then
      exit "$(tr -d '\n' < "$ROOT/build.exit")"
    fi
    exit 0
    ;;
esac

echo "unsupported fake engine command: $COMMAND $*" >&2
exit 1
"#;
            fs::write(&engine_path, script).expect("engine script");
            fs::write(&podman_engine_path, script).expect("podman engine script");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                let mut permissions = fs::metadata(&engine_path)
                    .expect("engine metadata")
                    .permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&engine_path, permissions.clone()).expect("engine permissions");
                fs::set_permissions(&podman_engine_path, permissions)
                    .expect("podman engine permissions");
            }

            Self {
                root,
                engine_path,
                podman_engine_path,
                invocation_log,
            }
        }

        fn write(&self, name: &str, contents: &str) {
            fs::write(self.root.join(name), contents).expect("fixture file");
        }

        fn args(&self) -> Vec<String> {
            vec![
                "--docker-path".to_string(),
                self.engine_path.display().to_string(),
            ]
        }

        fn args_with_podman_name(&self) -> Vec<String> {
            vec![
                "--docker-path".to_string(),
                self.podman_engine_path.display().to_string(),
            ]
        }

        fn invocations(&self) -> String {
            fs::read_to_string(&self.invocation_log).expect("invocations")
        }
    }

    impl Drop for FakeEngineFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn image_inspect_output(user: &str, platform: Option<&str>) -> String {
        format!("{user}\n{}\n", platform.unwrap_or_default())
    }
}
