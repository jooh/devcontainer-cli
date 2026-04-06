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
        build_outdated_payload, build_read_configuration_payload, run_upgrade_lockfile,
        should_use_native_read_configuration,
    };
    use crate::commands::common::resolve_read_configuration_path;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let unique_id = NEXT_TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "devcontainer-config-command-test-{}-{suffix}-{unique_id}",
            std::process::id()
        ))
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
    fn fails_when_workspace_folder_option_is_missing_a_value() {
        let result = resolve_read_configuration_path(&["--workspace-folder".to_string()]);

        assert_eq!(
            result.expect_err("expected missing option value"),
            "Missing value for option: --workspace-folder"
        );
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
    fn infers_workspace_root_for_nested_devcontainer_configs() {
        let root = unique_temp_dir();
        let nested_config_dir = root.join(".devcontainer").join("python");
        let config = nested_config_dir.join("devcontainer.json");
        fs::create_dir_all(&root).expect("failed to create root");
        fs::create_dir_all(&nested_config_dir).expect("failed to create nested config directory");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec!["--config".to_string(), config.display().to_string()];
        let (workspace_folder, config_file) =
            resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            workspace_folder,
            fs::canonicalize(&root).expect("failed to canonicalize workspace")
        );
        assert_eq!(
            config_file,
            fs::canonicalize(config).expect("failed to canonicalize config")
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
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": { \"ghcr.io/devcontainers/features/git:1\": {} }\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--include-merged-configuration".to_string(),
            "--include-features-configuration".to_string(),
        ];
        let payload = build_read_configuration_payload(&args).expect("payload");

        assert_eq!(payload["configuration"]["image"], "debian:bookworm");
        assert_eq!(payload["mergedConfiguration"]["image"], "debian:bookworm");
        assert!(payload["featuresConfiguration"]["features"]
            .as_object()
            .expect("features object")
            .contains_key("ghcr.io/devcontainers/features/git:1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn outdated_payload_reports_feature_versions() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/node:1\": {}\n  }\n}\n",
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
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/node:1\": {}\n  }\n}\n",
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
