use std::fs;
use std::path::Path;

use serde_json::{json, Map, Value};

use super::common;

const PICK_CONFIG_PROPERTIES: [&str; 20] = [
    "onCreateCommand",
    "updateContentCommand",
    "postCreateCommand",
    "postStartCommand",
    "postAttachCommand",
    "waitFor",
    "customizations",
    "mounts",
    "containerEnv",
    "containerUser",
    "init",
    "privileged",
    "capAdd",
    "securityOpt",
    "remoteUser",
    "userEnvProbe",
    "remoteEnv",
    "overrideCommand",
    "portsAttributes",
    "otherPortsAttributes",
];

const PICK_FEATURE_PROPERTIES: [&str; 10] = [
    "onCreateCommand",
    "updateContentCommand",
    "postCreateCommand",
    "postStartCommand",
    "postAttachCommand",
    "init",
    "privileged",
    "capAdd",
    "securityOpt",
    "customizations",
];

const REPLACE_PROPERTIES: [&str; 8] = [
    "customizations",
    "entrypoint",
    "onCreateCommand",
    "updateContentCommand",
    "postCreateCommand",
    "postStartCommand",
    "postAttachCommand",
    "shutdownAction",
];

fn pick_properties(value: &Value, keys: &[&str]) -> Map<String, Value> {
    let Some(object) = value.as_object() else {
        return Map::new();
    };

    keys.iter()
        .filter_map(|key| {
            object
                .get(*key)
                .map(|value| ((*key).to_string(), value.clone()))
        })
        .collect()
}

fn merge_legacy_feature_customizations(feature: &mut Map<String, Value>) {
    let extensions = feature.remove("extensions");
    let settings = feature.remove("settings");
    if extensions.is_none() && settings.is_none() {
        return;
    }

    let customizations = feature
        .entry("customizations".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(customizations_object) = customizations.as_object_mut() else {
        return;
    };
    let vscode = customizations_object
        .entry("vscode".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(vscode_object) = vscode.as_object_mut() else {
        return;
    };

    if let Some(Value::Array(mut extension_values)) = extensions {
        let entry = vscode_object
            .entry("extensions".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(existing) = entry.as_array_mut() {
            existing.append(&mut extension_values);
        }
    }

    if let Some(Value::Object(settings_object)) = settings {
        let entry = vscode_object
            .entry("settings".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(existing) = entry.as_object_mut() {
            let mut merged = settings_object;
            for (key, value) in existing.clone() {
                merged.insert(key, value);
            }
            *existing = merged;
        }
    }
}

fn build_features_configuration(
    workspace_folder: &Path,
    config_file: &Path,
    configuration: &Value,
) -> Result<Option<Value>, String> {
    let Some(features) = configuration.get("features").and_then(Value::as_object) else {
        return Ok(None);
    };

    let allowed_parent = fs::canonicalize(workspace_folder.join(".devcontainer"))
        .unwrap_or_else(|_| workspace_folder.join(".devcontainer"));
    let config_dir = config_file
        .parent()
        .ok_or_else(|| format!("config file has no parent: {}", config_file.display()))?;
    let mut feature_sets = Vec::new();

    for (user_feature_id, user_value) in features {
        let feature_path = Path::new(user_feature_id);
        if feature_path.is_absolute() || !user_feature_id.starts_with('.') {
            return Err(format!(
                "native read-configuration currently supports only local relative features for --include-features-configuration (unsupported: {user_feature_id})"
            ));
        }

        let feature_folder = fs::canonicalize(config_dir.join(feature_path))
            .unwrap_or_else(|_| config_dir.join(feature_path));
        if !feature_folder.starts_with(&allowed_parent) {
            return Err(format!(
                "local feature path must remain under {} (resolved: {})",
                allowed_parent.display(),
                feature_folder.display()
            ));
        }

        let metadata_path = feature_folder.join("devcontainer-feature.json");
        let feature_json = crate::config::parse_jsonc_value(
            &fs::read_to_string(&metadata_path).map_err(|error| error.to_string())?,
        )?;
        let mut feature = feature_json.as_object().cloned().ok_or_else(|| {
            format!(
                "feature metadata must be an object: {}",
                metadata_path.display()
            )
        })?;
        feature.insert(
            "id".to_string(),
            Value::String(
                feature_path
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| user_feature_id.to_string()),
            ),
        );
        feature.insert(
            "name".to_string(),
            Value::String(user_feature_id.to_string()),
        );
        feature.insert("value".to_string(), user_value.clone());
        feature.insert("included".to_string(), Value::Bool(true));
        merge_legacy_feature_customizations(&mut feature);

        feature_sets.push(json!({
            "sourceInformation": {
                "type": "file-path",
                "resolvedFilePath": feature_folder,
                "userFeatureId": user_feature_id,
            },
            "features": [Value::Object(feature)],
            "internalVersion": "2",
        }));
    }

    Ok(Some(json!({
        "featureSets": feature_sets,
    })))
}

fn merge_object_property(entries: &[Value], key: &str) -> Map<String, Value> {
    let mut merged = Map::new();
    for entry in entries {
        if let Some(values) = entry.get(key).and_then(Value::as_object) {
            for (name, value) in values {
                merged.insert(name.clone(), value.clone());
            }
        }
    }
    merged
}

fn collect_property_values(entries: &[Value], key: &str) -> Option<Value> {
    let collected: Vec<Value> = entries
        .iter()
        .filter_map(|entry| entry.get(key).cloned())
        .collect();
    if collected.is_empty() {
        None
    } else {
        Some(Value::Array(collected))
    }
}

fn merge_unique_array_property(entries: &[Value], key: &str) -> Option<Value> {
    let mut collected = Vec::new();
    for entry in entries {
        if let Some(values) = entry.get(key).and_then(Value::as_array) {
            for value in values {
                if !collected.iter().any(|existing: &Value| existing == value) {
                    collected.push(value.clone());
                }
            }
        }
    }
    if collected.is_empty() {
        None
    } else {
        Some(Value::Array(collected))
    }
}

fn find_last_property(entries: &[Value], key: &str) -> Option<Value> {
    entries
        .iter()
        .rev()
        .find_map(|entry| entry.get(key).cloned())
}

fn merge_customizations(entries: &[Value]) -> Option<Value> {
    let mut merged = Map::new();
    for entry in entries {
        let Some(customizations) = entry.get("customizations").and_then(Value::as_object) else {
            continue;
        };
        for (key, value) in customizations {
            let bucket = merged
                .entry(key.clone())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Some(values) = bucket.as_array_mut() {
                values.push(value.clone());
            }
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(Value::Object(merged))
    }
}

fn feature_metadata_entries(features_configuration: Option<&Value>) -> Vec<Value> {
    let mut entries = Vec::new();
    let Some(feature_sets) = features_configuration
        .and_then(|value| value.get("featureSets"))
        .and_then(Value::as_array)
    else {
        return entries;
    };

    for feature_set in feature_sets {
        let user_feature_id = feature_set
            .get("sourceInformation")
            .and_then(|value| value.get("userFeatureId"))
            .cloned();
        let Some(features) = feature_set.get("features").and_then(Value::as_array) else {
            continue;
        };
        for feature in features {
            let mut picked = pick_properties(feature, &PICK_FEATURE_PROPERTIES);
            if let Some(user_feature_id) = user_feature_id.clone() {
                picked.insert("id".to_string(), user_feature_id);
            }
            if !picked.is_empty() {
                entries.push(Value::Object(picked));
            }
        }
    }

    entries
}

fn merge_configuration(configuration: &Value, features_configuration: Option<&Value>) -> Value {
    let mut image_metadata = feature_metadata_entries(features_configuration);
    let config_entry = pick_properties(configuration, &PICK_CONFIG_PROPERTIES);
    if !config_entry.is_empty() {
        image_metadata.push(Value::Object(config_entry));
    }

    let mut merged = configuration.as_object().cloned().unwrap_or_default();
    for key in REPLACE_PROPERTIES {
        merged.remove(key);
    }

    merged.insert(
        "init".to_string(),
        Value::Bool(
            image_metadata
                .iter()
                .any(|entry| entry.get("init").and_then(Value::as_bool).unwrap_or(false)),
        ),
    );
    merged.insert(
        "privileged".to_string(),
        Value::Bool(image_metadata.iter().any(|entry| {
            entry
                .get("privileged")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })),
    );
    merged.insert(
        "remoteEnv".to_string(),
        Value::Object(merge_object_property(&image_metadata, "remoteEnv")),
    );
    merged.insert(
        "containerEnv".to_string(),
        Value::Object(merge_object_property(&image_metadata, "containerEnv")),
    );
    merged.insert(
        "portsAttributes".to_string(),
        Value::Object(merge_object_property(&image_metadata, "portsAttributes")),
    );

    if let Some(value) = merge_customizations(&image_metadata) {
        merged.insert("customizations".to_string(), value);
    }
    if let Some(value) = merge_unique_array_property(&image_metadata, "capAdd") {
        merged.insert("capAdd".to_string(), value);
    }
    if let Some(value) = merge_unique_array_property(&image_metadata, "securityOpt") {
        merged.insert("securityOpt".to_string(), value);
    }
    if let Some(value) = collect_property_values(&image_metadata, "entrypoint") {
        merged.insert("entrypoints".to_string(), value);
    }
    if let Some(value) = collect_property_values(&image_metadata, "onCreateCommand") {
        merged.insert("onCreateCommands".to_string(), value);
    }
    if let Some(value) = collect_property_values(&image_metadata, "updateContentCommand") {
        merged.insert("updateContentCommands".to_string(), value);
    }
    if let Some(value) = collect_property_values(&image_metadata, "postCreateCommand") {
        merged.insert("postCreateCommands".to_string(), value);
    }
    if let Some(value) = collect_property_values(&image_metadata, "postStartCommand") {
        merged.insert("postStartCommands".to_string(), value);
    }
    if let Some(value) = collect_property_values(&image_metadata, "postAttachCommand") {
        merged.insert("postAttachCommands".to_string(), value);
    }
    if let Some(value) = find_last_property(&image_metadata, "waitFor") {
        merged.insert("waitFor".to_string(), value);
    }
    if let Some(value) = find_last_property(&image_metadata, "remoteUser") {
        merged.insert("remoteUser".to_string(), value);
    }
    if let Some(value) = find_last_property(&image_metadata, "containerUser") {
        merged.insert("containerUser".to_string(), value);
    }
    if let Some(value) = find_last_property(&image_metadata, "userEnvProbe") {
        merged.insert("userEnvProbe".to_string(), value);
    }
    if let Some(value) = find_last_property(&image_metadata, "overrideCommand") {
        merged.insert("overrideCommand".to_string(), value);
    }
    if let Some(value) = find_last_property(&image_metadata, "otherPortsAttributes") {
        merged.insert("otherPortsAttributes".to_string(), value);
    }
    if let Some(value) = find_last_property(&image_metadata, "shutdownAction") {
        merged.insert("shutdownAction".to_string(), value);
    }

    Value::Object(merged)
}

pub(crate) fn build_read_configuration_payload(args: &[String]) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    let mut payload = Map::new();
    let features_configuration = if common::has_flag(args, "--include-features-configuration")
        || common::has_flag(args, "--include-merged-configuration")
    {
        build_features_configuration(&workspace_folder, &config_file, &configuration)?
    } else {
        None
    };

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
        payload.insert(
            "mergedConfiguration".to_string(),
            merge_configuration(&configuration, features_configuration.as_ref()),
        );
    }

    if common::has_flag(args, "--include-features-configuration") {
        if let Some(features_configuration) = features_configuration {
            payload.insert("featuresConfiguration".to_string(), features_configuration);
        }
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
        "mergedConfiguration": if common::has_flag(args, "--include-merged-configuration") { merge_configuration(&configuration, None) } else { Value::Null },
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
    use serde_json::Value;
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
        let feature_dir = root.join(".devcontainer").join("localFeatureA");
        fs::create_dir_all(&feature_dir).expect("failed to create feature directory");
        fs::write(
            root.join(".devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"postCreateCommand\": \"echo ready\",\n  \"features\": { \"./.devcontainer/localFeatureA\": { \"greeting\": \"hello\" } }\n}\n",
        )
        .expect("failed to write config");
        fs::write(
            feature_dir.join("devcontainer-feature.json"),
            "{\n  \"id\": \"localFeatureA\",\n  \"version\": \"1.0.0\",\n  \"customizations\": {\n    \"vscode\": {\n      \"extensions\": [\"dbaeumer.vscode-eslint\"]\n    }\n  }\n}\n",
        )
        .expect("failed to write feature metadata");

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
            payload["mergedConfiguration"]["postCreateCommands"][0],
            "echo ready"
        );
        assert!(payload["mergedConfiguration"]
            .get("postCreateCommand")
            .is_none());
        assert_eq!(payload["mergedConfiguration"]["init"], Value::Bool(false));
        assert_eq!(
            payload["featuresConfiguration"]["featureSets"][0]["features"][0]["id"],
            "localFeatureA"
        );
        assert_eq!(
            payload["featuresConfiguration"]["featureSets"][0]["features"][0]["customizations"]
                ["vscode"]["extensions"][0],
            "dbaeumer.vscode-eslint"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_configuration_feature_payload_normalizes_legacy_customizations() {
        let root = unique_temp_dir();
        let feature_dir = root.join(".devcontainer").join("localFeatureB");
        fs::create_dir_all(&feature_dir).expect("failed to create feature directory");
        fs::write(
            root.join(".devcontainer.json"),
            "{\n  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\",\n  \"features\": { \"./.devcontainer/localFeatureB\": { \"favorite\": \"gold\" } }\n}\n",
        )
        .expect("failed to write config");
        fs::write(
            feature_dir.join("devcontainer-feature.json"),
            "{\n  \"id\": \"localFeatureB\",\n  \"version\": \"1.0.0\",\n  \"extensions\": [\"ms-dotnettools.csharp\"],\n  \"settings\": {\n    \"files.watcherExclude\": {\n      \"**/test/**\": true\n    }\n  }\n}\n",
        )
        .expect("failed to write feature metadata");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--include-features-configuration".to_string(),
        ];
        let payload = build_read_configuration_payload(&args).expect("payload");

        assert_eq!(
            payload["featuresConfiguration"]["featureSets"][0]["features"][0]["customizations"]
                ["vscode"]["extensions"][0],
            "ms-dotnettools.csharp"
        );
        assert_eq!(
            payload["featuresConfiguration"]["featureSets"][0]["features"][0]["customizations"]
                ["vscode"]["settings"]["files.watcherExclude"]["**/test/**"],
            Value::Bool(true)
        );
        assert!(
            payload["featuresConfiguration"]["featureSets"][0]["features"][0]
                .get("extensions")
                .is_none()
        );
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
