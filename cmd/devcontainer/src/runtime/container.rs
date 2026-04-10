use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::commands::common;

use super::compose;
use super::context::{derived_workspace_mount, workspace_mount_for_args, ResolvedConfig};
use super::engine;
use super::lifecycle::LifecycleMode;
use super::metadata::serialized_container_metadata;

pub(crate) struct UpContainer {
    pub(crate) container_id: String,
    pub(crate) lifecycle_mode: LifecycleMode,
}

static NEXT_UID_UPDATE_BUILD_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn prepare_up_image(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
) -> Result<String, String> {
    if !should_update_remote_user_uid(
        &resolved.configuration,
        args,
        is_uid_update_platform_supported(),
    ) {
        return Ok(image_name.to_string());
    }

    let remote_user = configured_updatable_user(&resolved.configuration)
        .ok_or_else(|| "Expected a non-root remote user for UID update".to_string())?;
    let image_user = resolved
        .configuration
        .get("containerUser")
        .or_else(|| resolved.configuration.get("remoteUser"))
        .and_then(Value::as_str)
        .unwrap_or("root");
    let (new_uid, new_gid) = host_uid_gid()?;
    let updated_image_name = format!("{image_name}-uid");
    let build_context = unique_uid_update_build_context();
    fs::create_dir_all(&build_context).map_err(|error| error.to_string())?;
    let dockerfile = uid_update_dockerfile_path();
    let mut build_args = vec![
        "build".to_string(),
        "--build-arg".to_string(),
        format!("BASE_IMAGE={image_name}"),
        "--build-arg".to_string(),
        format!("REMOTE_USER={remote_user}"),
        "--build-arg".to_string(),
        format!("NEW_UID={new_uid}"),
        "--build-arg".to_string(),
        format!("NEW_GID={new_gid}"),
        "--build-arg".to_string(),
        format!("IMAGE_USER={image_user}"),
        "-t".to_string(),
        updated_image_name.clone(),
        "-f".to_string(),
        dockerfile.display().to_string(),
        build_context.display().to_string(),
    ];

    let result = engine::run_engine(args, std::mem::take(&mut build_args))?;
    let _ = fs::remove_dir_all(&build_context);
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    Ok(updated_image_name)
}

pub(crate) fn ensure_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    if compose::uses_compose_config(&resolved.configuration) {
        return ensure_compose_up_container(resolved, args, image_name, remote_workspace_folder);
    }

    ensure_engine_up_container(resolved, args, image_name, remote_workspace_folder)
}

fn ensure_compose_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    let remove_existing = common::has_flag(args, "--remove-existing-container");
    if let Some(container_id) = compose::resolve_container_id(resolved, args)? {
        if remove_existing {
            compose::remove_service(resolved, args)?;
            return create_compose_container(resolved, args, image_name, remote_workspace_folder);
        }
        return refresh_compose_container(
            resolved,
            args,
            image_name,
            remote_workspace_folder,
            &container_id,
            LifecycleMode::UpReused,
        );
    }

    if let Some(container_id) = compose::resolve_container_id_including_stopped(resolved, args)? {
        if remove_existing {
            compose::remove_service(resolved, args)?;
            return create_compose_container(resolved, args, image_name, remote_workspace_folder);
        }
        return refresh_compose_container(
            resolved,
            args,
            image_name,
            remote_workspace_folder,
            &container_id,
            LifecycleMode::UpStarted,
        );
    }

    if common::has_flag(args, "--expect-existing-container") {
        return Err("Dev container not found.".to_string());
    }

    create_compose_container(resolved, args, image_name, remote_workspace_folder)
}

fn ensure_engine_up_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    let running = find_target_container(
        args,
        Some(resolved.workspace_folder.as_path()),
        Some(resolved.config_file.as_path()),
        false,
    )?;
    let remove_existing = common::has_flag(args, "--remove-existing-container");
    match running {
        Some(container_id) if remove_existing => {
            remove_container(args, &container_id)?;
            create_engine_container(resolved, args, image_name, remote_workspace_folder)
        }
        Some(container_id) => Ok(UpContainer {
            container_id,
            lifecycle_mode: LifecycleMode::UpReused,
        }),
        None => match find_target_container(
            args,
            Some(resolved.workspace_folder.as_path()),
            Some(resolved.config_file.as_path()),
            true,
        )? {
            Some(container_id) if remove_existing => {
                remove_container(args, &container_id)?;
                create_engine_container(resolved, args, image_name, remote_workspace_folder)
            }
            Some(container_id) => {
                start_existing_container(args, &container_id)?;
                Ok(UpContainer {
                    container_id,
                    lifecycle_mode: LifecycleMode::UpStarted,
                })
            }
            None if common::has_flag(args, "--expect-existing-container") => {
                Err("Dev container not found.".to_string())
            }
            None => create_engine_container(resolved, args, image_name, remote_workspace_folder),
        },
    }
}

fn create_compose_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    compose::up_service(resolved, args, remote_workspace_folder, image_name)?;
    let container_id = compose::resolve_container_id(resolved, args)?
        .ok_or_else(|| "Dev container not found.".to_string())?;
    Ok(UpContainer {
        container_id,
        lifecycle_mode: LifecycleMode::UpCreated,
    })
}

fn refresh_compose_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
    previous_container_id: &str,
    unchanged_mode: LifecycleMode,
) -> Result<UpContainer, String> {
    compose::up_service(resolved, args, remote_workspace_folder, image_name)?;
    let updated_container_id = compose::resolve_container_id(resolved, args)?
        .ok_or_else(|| "Dev container not found.".to_string())?;
    Ok(UpContainer {
        lifecycle_mode: if updated_container_id == previous_container_id {
            unchanged_mode
        } else {
            LifecycleMode::UpCreated
        },
        container_id: updated_container_id,
    })
}

fn create_engine_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<UpContainer, String> {
    start_container(resolved, args, image_name, remote_workspace_folder).map(|container_id| {
        UpContainer {
            container_id,
            lifecycle_mode: LifecycleMode::UpCreated,
        }
    })
}

pub(crate) fn resolve_target_container(
    args: &[String],
    workspace_folder: Option<&std::path::Path>,
    config_file: Option<&std::path::Path>,
) -> Result<String, String> {
    if let Some(container_id) = common::parse_option_value(args, "--container-id") {
        return Ok(container_id);
    }

    match find_target_container(args, workspace_folder, config_file, false)? {
        Some(container_id) => Ok(container_id),
        None => Err("Dev container not found.".to_string()),
    }
}

fn start_container(
    resolved: &ResolvedConfig,
    args: &[String],
    image_name: &str,
    remote_workspace_folder: &str,
) -> Result<String, String> {
    let mut engine_args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--label".to_string(),
        format!(
            "devcontainer.local_folder={}",
            resolved.workspace_folder.display()
        ),
        "--label".to_string(),
        format!(
            "devcontainer.config_file={}",
            resolved.config_file.display()
        ),
        "--label".to_string(),
        format!(
            "devcontainer.metadata={}",
            serialized_container_metadata(
                &resolved.configuration,
                remote_workspace_folder,
                common::runtime_options(args).omit_config_remote_env_from_metadata,
            )?
        ),
        "--mount".to_string(),
        workspace_mount_for_args(resolved, remote_workspace_folder, args),
    ];
    if resolved.configuration.get("workspaceMount").is_none() {
        if let Some(derived) = derived_workspace_mount(&resolved.workspace_folder, args) {
            for mount in derived.additional_mounts {
                engine_args.push("--mount".to_string());
                engine_args.push(mount);
            }
        }
    }
    if resolved
        .configuration
        .get("init")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        engine_args.push("--init".to_string());
    }
    if resolved
        .configuration
        .get("privileged")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        engine_args.push("--privileged".to_string());
    }
    for label in common::parse_option_values(args, "--id-label") {
        engine_args.push("--label".to_string());
        engine_args.push(label);
    }
    for mount in common::parse_option_values(args, "--mount") {
        engine_args.push("--mount".to_string());
        engine_args.push(mount);
    }
    if let Some(mounts) = resolved
        .configuration
        .get("mounts")
        .and_then(Value::as_array)
    {
        for mount in mounts.iter().filter_map(mount_argument) {
            engine_args.push("--mount".to_string());
            engine_args.push(mount);
        }
    }
    if let Some(run_args) = resolved
        .configuration
        .get("runArgs")
        .and_then(Value::as_array)
    {
        for arg in run_args.iter().filter_map(Value::as_str) {
            engine_args.push(arg.to_string());
        }
    }
    if let Some(container_env) = resolved
        .configuration
        .get("containerEnv")
        .and_then(Value::as_object)
    {
        for (key, value) in container_env {
            if let Some(value) = value.as_str() {
                engine_args.push("-e".to_string());
                engine_args.push(format!("{key}={value}"));
            }
        }
    }
    if let Some(cap_add) = resolved
        .configuration
        .get("capAdd")
        .and_then(Value::as_array)
    {
        for capability in cap_add.iter().filter_map(Value::as_str) {
            engine_args.push("--cap-add".to_string());
            engine_args.push(capability.to_string());
        }
    }
    if let Some(security_opt) = resolved
        .configuration
        .get("securityOpt")
        .and_then(Value::as_array)
    {
        for option in security_opt.iter().filter_map(Value::as_str) {
            engine_args.push("--security-opt".to_string());
            engine_args.push(option.to_string());
        }
    }
    if should_add_gpu_capability(&resolved.configuration, args)? {
        engine_args.push("--gpus".to_string());
        engine_args.push("all".to_string());
    }
    engine_args.push(image_name.to_string());
    engine_args.push("/bin/sh".to_string());
    engine_args.push("-lc".to_string());
    engine_args.push("while sleep 1000; do :; done".to_string());

    let result = engine::run_engine(args, engine_args)?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    let container_id = result.stdout.trim().to_string();
    if container_id.is_empty() {
        return Err("Container engine did not return a container id".to_string());
    }

    Ok(container_id)
}

pub(crate) fn should_add_gpu_capability(
    configuration: &Value,
    args: &[String],
) -> Result<bool, String> {
    if configuration
        .get("hostRequirements")
        .and_then(|requirements| requirements.get("gpu"))
        .is_none()
    {
        return Ok(false);
    }

    match common::runtime_options(args).gpu_availability.as_deref() {
        Some("all") => Ok(true),
        Some("none") => Ok(false),
        _ => detect_gpu_support(args),
    }
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

    configured_updatable_user(configuration).is_some()
}

fn detect_gpu_support(args: &[String]) -> Result<bool, String> {
    let result = engine::run_engine(
        args,
        vec![
            "info".to_string(),
            "-f".to_string(),
            "{{.Runtimes.nvidia}}".to_string(),
        ],
    )?;
    if result.status_code != 0 {
        return Ok(false);
    }
    Ok(result.stdout.contains("nvidia-container-runtime"))
}

fn configured_updatable_user(configuration: &Value) -> Option<&str> {
    configuration
        .get("remoteUser")
        .or_else(|| configuration.get("containerUser"))
        .and_then(Value::as_str)
        .filter(|user| *user != "root" && !user.chars().all(|character| character.is_ascii_digit()))
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
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let unique_id = NEXT_UID_UPDATE_BUILD_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "devcontainer-update-uid-{}-{suffix}-{unique_id}",
        std::process::id()
    ))
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

fn mount_argument(value: &Value) -> Option<String> {
    match value {
        Value::String(mount) => Some(mount.clone()),
        Value::Object(entries) => {
            let mut options = Vec::new();
            if let Some(value) = entries.get("type").and_then(mount_option_value) {
                options.push(format!("type={value}"));
            }
            if let Some(value) = entries
                .get("source")
                .or_else(|| entries.get("src"))
                .and_then(mount_option_value)
            {
                options.push(format!("source={value}"));
            }
            if let Some(value) = entries
                .get("target")
                .or_else(|| entries.get("destination"))
                .or_else(|| entries.get("dst"))
                .and_then(mount_option_value)
            {
                options.push(format!("target={value}"));
            }
            if entries
                .get("readonly")
                .or_else(|| entries.get("readOnly"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                options.push("readonly".to_string());
            }
            for (key, value) in entries {
                if matches!(
                    key.as_str(),
                    "type"
                        | "source"
                        | "src"
                        | "target"
                        | "destination"
                        | "dst"
                        | "readonly"
                        | "readOnly"
                ) {
                    continue;
                }
                if let Some(value) = mount_option_value(value) {
                    options.push(format!("{key}={value}"));
                }
            }
            (!options.is_empty()).then(|| options.join(","))
        }
        _ => None,
    }
}

fn mount_option_value(value: &Value) -> Option<String> {
    match value {
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

fn start_existing_container(args: &[String], container_id: &str) -> Result<(), String> {
    let result = engine::run_engine(args, vec!["start".to_string(), container_id.to_string()])?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

fn remove_container(args: &[String], container_id: &str) -> Result<(), String> {
    let result = engine::run_engine(
        args,
        vec!["rm".to_string(), "-f".to_string(), container_id.to_string()],
    )?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

fn find_target_container(
    args: &[String],
    workspace_folder: Option<&std::path::Path>,
    config_file: Option<&std::path::Path>,
    include_stopped: bool,
) -> Result<Option<String>, String> {
    let labels = target_container_labels(args, workspace_folder, config_file);
    if labels.is_empty() {
        return Err(
            "Unable to determine target container. Provide --container-id or --workspace-folder."
                .to_string(),
        );
    }

    let mut engine_args = vec!["ps".to_string(), "-q".to_string()];
    if include_stopped {
        engine_args.push("-a".to_string());
    }
    for label in labels {
        engine_args.push("--filter".to_string());
        engine_args.push(format!("label={label}"));
    }

    let result = engine::run_engine(args, engine_args)?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    Ok(result
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.chars().any(char::is_whitespace))
        .map(str::to_string))
}

fn target_container_labels(
    args: &[String],
    workspace_folder: Option<&std::path::Path>,
    config_file: Option<&std::path::Path>,
) -> Vec<String> {
    let mut labels = common::parse_option_values(args, "--id-label");
    if labels.is_empty() {
        if let Some(workspace_folder) = workspace_folder {
            labels.push(format!(
                "devcontainer.local_folder={}",
                workspace_folder.display()
            ));
        }
        if let Some(config_file) = config_file {
            labels.push(format!(
                "devcontainer.config_file={}",
                config_file.display()
            ));
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{mount_argument, should_update_remote_user_uid};

    #[test]
    fn mount_argument_preserves_read_only_and_alias_keys() {
        let mount = mount_argument(&json!({
            "type": "bind",
            "src": "/cache",
            "dst": "/workspace/cache",
            "readOnly": true,
        }))
        .expect("mount argument");

        assert_eq!(
            mount,
            "type=bind,source=/cache,target=/workspace/cache,readonly"
        );
    }

    #[test]
    fn mount_argument_preserves_additional_scalar_options() {
        let mount = mount_argument(&json!({
            "type": "volume",
            "source": "devcontainer-cache",
            "target": "/cache",
            "external": true,
            "consistency": "delegated",
        }))
        .expect("mount argument");

        assert_eq!(
            mount,
            "type=volume,source=devcontainer-cache,target=/cache,consistency=delegated,external=true"
        );
    }

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
}
