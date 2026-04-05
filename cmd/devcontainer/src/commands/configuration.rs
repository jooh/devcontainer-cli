use std::fs;

use serde_json::{json, Map, Value};

use super::common;

pub(crate) fn build_read_configuration_payload(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    let mut payload = Map::new();
    payload.insert("configuration".to_string(), configuration.clone());
    payload.insert(
        "metadata".to_string(),
        json!({
            "format": "jsonc",
            "pathResolution": "native-rust",
            "workspaceFolder": workspace_folder,
            "configFile": config_file,
        }),
    );

    if common::has_flag(args, "--include-merged-configuration") {
        payload.insert("mergedConfiguration".to_string(), configuration.clone());
    }

    if common::has_flag(args, "--include-features-configuration") {
        payload.insert(
            "featuresConfiguration".to_string(),
            json!({
                "features": configuration.get("features").cloned().unwrap_or_else(|| json!({})),
            }),
        );
    }

    Ok(Value::Object(payload))
}

pub(crate) fn should_use_native_read_configuration(args: &[String]) -> bool {
    const SUPPORTED_OPTIONS: [&str; 4] = [
        "--workspace-folder",
        "--config",
        "--include-merged-configuration",
        "--include-features-configuration",
    ];
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with("--") {
            return false;
        }
        if !SUPPORTED_OPTIONS.contains(&arg.as_str()) {
            return false;
        }
        index += if matches!(
            arg.as_str(),
            "--include-merged-configuration" | "--include-features-configuration"
        ) {
            1
        } else {
            2
        };
    }
    true
}

pub(crate) fn build_build_payload(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    let build_section = configuration
        .get("build")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let dockerfile = build_section
        .get("dockerfile")
        .or_else(|| build_section.get("dockerFile"))
        .and_then(Value::as_str)
        .unwrap_or("Dockerfile");
    let context = build_section
        .get("context")
        .and_then(Value::as_str)
        .unwrap_or(".");

    let mut docker_args = vec!["build".to_string()];
    if common::has_flag(args, "--no-cache") {
        docker_args.push("--no-cache".to_string());
    }
    for value in common::parse_option_values(args, "--cache-from") {
        docker_args.push("--cache-from".to_string());
        docker_args.push(value);
    }
    for value in common::parse_option_values(args, "--cache-to") {
        docker_args.push("--cache-to".to_string());
        docker_args.push(value);
    }
    for value in common::parse_option_values(args, "--label") {
        docker_args.push("--label".to_string());
        docker_args.push(value);
    }
    if let Some(image_name) = common::parse_option_value(args, "--image-name") {
        docker_args.push("--tag".to_string());
        docker_args.push(image_name);
    }
    if let Some(platform) = common::parse_option_value(args, "--platform") {
        docker_args.push("--platform".to_string());
        docker_args.push(platform);
    }
    docker_args.push("--file".to_string());
    docker_args.push(dockerfile.to_string());
    docker_args.push(context.to_string());

    Ok(json!({
        "outcome": "success",
        "command": "build",
        "workspaceFolder": workspace_folder,
        "configFile": config_file,
        "buildKit": common::parse_option_value(args, "--buildkit").unwrap_or_else(|| "auto".to_string()),
        "push": common::has_flag(args, "--push"),
        "docker": {
            "program": "docker",
            "args": docker_args,
        },
        "configuration": configuration,
    }))
}

pub(crate) fn build_lifecycle_payload(command: &str, args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    Ok(json!({
        "outcome": "success",
        "command": command,
        "workspaceFolder": workspace_folder,
        "configFile": config_file,
        "mounts": common::parse_mounts(args),
        "remoteEnv": common::parse_remote_env(args),
        "skipPostCreate": common::has_flag(args, "--skip-post-create"),
        "skipPostAttach": common::has_flag(args, "--skip-post-attach"),
        "skipNonBlockingCommands": common::has_flag(args, "--skip-non-blocking-commands"),
        "lifecycleCommands": common::lifecycle_commands(&configuration),
        "configuration": if common::has_flag(args, "--include-configuration") { configuration.clone() } else { Value::Null },
        "mergedConfiguration": if common::has_flag(args, "--include-merged-configuration") { configuration.clone() } else { Value::Null },
    }))
}

pub(crate) fn build_outdated_payload(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let feature_versions: Map<String, Value> = features
        .keys()
        .map(|feature_id| {
            let current_version = feature_id
                .rsplit(':')
                .next()
                .filter(|version| *version != feature_id)
                .unwrap_or("unversioned");
            (
                feature_id.clone(),
                json!({
                    "currentVersion": current_version,
                    "latestVersion": "unknown",
                }),
            )
        })
        .collect();

    Ok(json!({
        "outcome": "success",
        "command": "outdated",
        "workspaceFolder": workspace_folder,
        "configFile": config_file,
        "features": feature_versions,
    }))
}

pub(crate) fn run_upgrade_lockfile(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, _, configuration) = common::load_resolved_config(args)?;
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let filtered_feature = common::parse_option_value(args, "--feature");
    let target_version = common::parse_option_value(args, "--target-version");
    let lockfile_features: Map<String, Value> = features
        .keys()
        .filter(|feature_id| {
            filtered_feature
                .as_ref()
                .map(|requested| feature_id.contains(requested))
                .unwrap_or(true)
        })
        .map(|feature_id| {
            let current_version = feature_id
                .rsplit(':')
                .next()
                .filter(|version| *version != feature_id)
                .unwrap_or("unversioned");
            (
                feature_id.clone(),
                json!({
                    "version": target_version.clone().unwrap_or_else(|| current_version.to_string()),
                }),
            )
        })
        .collect();

    let lockfile = json!({
        "features": lockfile_features,
    });

    if !common::has_flag(args, "--dry-run") {
        let lockfile_path = workspace_folder
            .join(".devcontainer")
            .join("devcontainer-lock.json");
        if let Some(parent) = lockfile_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(
            &lockfile_path,
            serde_json::to_string_pretty(&lockfile).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(json!({
        "outcome": "success",
        "command": "upgrade",
        "lockfile": lockfile,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        build_build_payload, build_lifecycle_payload, build_outdated_payload,
        build_read_configuration_payload, run_upgrade_lockfile,
        should_use_native_read_configuration,
    };
    use crate::commands::common::resolve_read_configuration_path;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("devcontainer-config-command-test-{suffix}"))
    }

    #[test]
    fn resolves_modern_config_path_from_workspace_folder() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        let config = config_dir.join("devcontainer.json");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
        let result = resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            result.1,
            fs::canonicalize(config).expect("failed to canonicalize")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fails_when_explicit_config_file_is_missing() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create root");
        let missing_config = root.join("missing.json");
        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--config".to_string(),
            missing_config.display().to_string(),
        ];

        let result = resolve_read_configuration_path(&args);

        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_relative_config_against_workspace_folder() {
        let root = unique_temp_dir();
        let config = root.join("relative.devcontainer.json");
        fs::create_dir_all(&root).expect("failed to create root");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--config".to_string(),
            "relative.devcontainer.json".to_string(),
        ];
        let result = resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            result.1,
            fs::canonicalize(config).expect("failed to canonicalize")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_configuration_with_additional_flags_is_supported_natively() {
        assert!(should_use_native_read_configuration(&[
            "--workspace-folder".to_string(),
            "/workspace".to_string(),
            "--include-merged-configuration".to_string(),
        ]));
    }

    #[test]
    fn read_configuration_payload_includes_optional_sections() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": { \"ghcr.io/devcontainers/features/git:1\": {} }\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--include-merged-configuration".to_string(),
            "--include-features-configuration".to_string(),
        ];
        let payload = build_read_configuration_payload(&args).expect("payload");

        assert_eq!(
            payload["configuration"]["image"],
            "mcr.microsoft.com/devcontainers/base:ubuntu"
        );
        assert_eq!(
            payload["mergedConfiguration"]["image"],
            "mcr.microsoft.com/devcontainers/base:ubuntu"
        );
        assert!(payload["featuresConfiguration"]["features"]
            .as_object()
            .expect("features object")
            .contains_key("ghcr.io/devcontainers/features/git:1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn build_payload_contains_docker_plan_and_flags() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"build\": {\n    \"dockerfile\": \"Dockerfile\",\n    \"context\": \"..\"\n  }\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--buildkit".to_string(),
            "never".to_string(),
            "--no-cache".to_string(),
            "--cache-from".to_string(),
            "ghcr.io/example/cache".to_string(),
            "--label".to_string(),
            "devcontainer.test=true".to_string(),
        ];
        let payload = build_build_payload(&args).expect("payload");
        let docker_args = payload["docker"]["args"].as_array().expect("docker args");

        assert!(docker_args.iter().any(|value| value == "--no-cache"));
        assert!(docker_args
            .iter()
            .any(|value| value == "ghcr.io/example/cache"));
        assert!(docker_args
            .iter()
            .any(|value| value == "devcontainer.test=true"));
        assert_eq!(payload["buildKit"], "never");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lifecycle_payload_collects_commands_mounts_and_remote_env() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"onCreateCommand\": \"echo create\",\n  \"postCreateCommand\": \"echo post\"\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--mount".to_string(),
            "type=bind,source=/tmp,target=/workspace".to_string(),
            "--remote-env".to_string(),
            "HELLO=world".to_string(),
        ];
        let payload = build_lifecycle_payload("up", &args).expect("payload");

        assert_eq!(payload["command"], "up");
        assert_eq!(payload["mounts"].as_array().expect("mounts").len(), 1);
        assert_eq!(payload["remoteEnv"]["HELLO"], "world");
        assert_eq!(
            payload["lifecycleCommands"]
                .as_array()
                .expect("lifecycle commands")
                .len(),
            2
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn outdated_payload_reports_feature_versions() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/node:1\": {}\n  }\n}\n",
        )
        .expect("failed to write config");

        let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
        let payload = build_outdated_payload(&args).expect("payload");

        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/node:1"]["currentVersion"],
            "1"
        );
        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/node:1"]["latestVersion"],
            "unknown"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upgrade_lockfile_writes_devcontainer_lock_json() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/node:1\": {}\n  }\n}\n",
        )
        .expect("failed to write config");

        let payload =
            run_upgrade_lockfile(&["--workspace-folder".to_string(), root.display().to_string()])
                .expect("lockfile payload");

        let lockfile = config_dir.join("devcontainer-lock.json");
        assert!(lockfile.is_file());
        assert_eq!(
            payload["lockfile"]["features"]["ghcr.io/devcontainers/features/node:1"]["version"],
            "1"
        );
        let _ = fs::remove_dir_all(root);
    }
}
