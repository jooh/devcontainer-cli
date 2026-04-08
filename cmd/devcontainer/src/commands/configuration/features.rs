use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::commands::collections::registry::{
    collection_slug, normalize_collection_reference, published_feature_install_script,
    published_feature_manifest,
};
use crate::commands::common;

#[derive(Clone, Debug)]
pub(crate) enum FeatureInstallationSource {
    Local(PathBuf),
    Published(String),
}

#[derive(Clone, Debug)]
pub(crate) struct FeatureInstallation {
    pub(crate) source: FeatureInstallationSource,
    pub(crate) env: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedFeatureSupport {
    pub(crate) features_configuration: Value,
    pub(crate) metadata_entries: Vec<Value>,
    pub(crate) installations: Vec<FeatureInstallation>,
    pub(crate) ordered_feature_ids: Vec<String>,
}

#[derive(Clone)]
struct FeatureSpec {
    manifest: Value,
    options: Value,
    source_information: Value,
    metadata_entry: Value,
    installation: FeatureInstallation,
    depends_on: Vec<String>,
}

pub(crate) fn resolve_feature_support(
    args: &[String],
    workspace_folder: &Path,
    config_file: &Path,
    configuration: &Value,
) -> Result<Option<ResolvedFeatureSupport>, String> {
    let declared = declared_features(args, configuration)?;
    if declared.is_empty() {
        return Ok(None);
    }

    let ordered_ids = resolve_feature_install_order(
        &declared,
        configuration,
        config_file.parent().unwrap_or(workspace_folder),
    )?;
    let mut feature_sets = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut installations = Vec::new();

    for feature_id in &ordered_ids {
        let feature_value = declared
            .get(feature_id)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        let spec = resolve_feature_spec(
            feature_id,
            &feature_value,
            config_file.parent().unwrap_or(workspace_folder),
        )?;
        feature_sets.push(json!({
            "features": [feature_object(&spec.manifest, &spec.options, &feature_value)],
            "internalVersion": "2",
            "sourceInformation": spec.source_information,
        }));
        if spec
            .metadata_entry
            .as_object()
            .is_some_and(|entries| !entries.is_empty())
        {
            metadata_entries.push(spec.metadata_entry);
        }
        installations.push(spec.installation);
    }

    Ok(Some(ResolvedFeatureSupport {
        features_configuration: json!({
            "featureSets": feature_sets,
        }),
        metadata_entries,
        installations,
        ordered_feature_ids: ordered_ids,
    }))
}

fn declared_features(args: &[String], configuration: &Value) -> Result<Map<String, Value>, String> {
    let mut declared = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(raw_additional) = common::parse_option_value(args, "--additional-features") {
        let additional = crate::config::parse_jsonc_value(&raw_additional)?;
        let additional = additional
            .as_object()
            .ok_or_else(|| "--additional-features must be a JSON object".to_string())?;
        for (key, value) in additional {
            declared.insert(key.clone(), value.clone());
        }
    }
    Ok(declared)
}

fn resolve_feature_install_order(
    declared: &Map<String, Value>,
    configuration: &Value,
    config_root: &Path,
) -> Result<Vec<String>, String> {
    let mut explicit_order = configuration
        .get("overrideFeatureInstallOrder")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .filter(|entry| declared.contains_key(*entry))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let remaining = declared
        .keys()
        .filter(|key| !explicit_order.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    explicit_order.extend(remaining);

    let mut ordered = Vec::new();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let mut cache = HashMap::new();
    for feature_id in explicit_order {
        visit_feature(
            &feature_id,
            declared,
            config_root,
            &mut cache,
            &mut visiting,
            &mut visited,
            &mut ordered,
        )?;
    }
    Ok(ordered)
}

fn visit_feature(
    feature_id: &str,
    declared: &Map<String, Value>,
    config_root: &Path,
    cache: &mut HashMap<String, FeatureSpec>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    ordered: &mut Vec<String>,
) -> Result<(), String> {
    if visited.contains(feature_id) {
        return Ok(());
    }
    if !visiting.insert(feature_id.to_string()) {
        return Err(format!(
            "Detected cyclic feature dependency at {feature_id}"
        ));
    }

    let spec = if let Some(spec) = cache.get(feature_id) {
        spec.clone()
    } else {
        let value = declared
            .get(feature_id)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        let spec = resolve_feature_spec(feature_id, &value, config_root)?;
        cache.insert(feature_id.to_string(), spec.clone());
        spec
    };

    for dependency in &spec.depends_on {
        visit_feature(
            dependency,
            declared,
            config_root,
            cache,
            visiting,
            visited,
            ordered,
        )?;
    }

    visiting.remove(feature_id);
    visited.insert(feature_id.to_string());
    ordered.push(feature_id.to_string());
    Ok(())
}

fn resolve_feature_spec(
    feature_id: &str,
    value: &Value,
    config_root: &Path,
) -> Result<FeatureSpec, String> {
    let (manifest, source_information, installation) = if is_local_feature_reference(feature_id) {
        let feature_dir = resolve_local_feature_path(config_root, feature_id);
        let manifest = common::parse_manifest(&feature_dir, "devcontainer-feature.json")?;
        let source_information = json!({
            "type": "file-path",
            "resolvedFilePath": feature_dir.display().to_string(),
            "userFeatureId": feature_id,
        });
        let installation = FeatureInstallation {
            source: FeatureInstallationSource::Local(feature_dir),
            env: feature_option_values_from_manifest(&manifest, value),
        };
        (manifest, source_information, installation)
    } else {
        let manifest = published_feature_manifest(feature_id).unwrap_or_else(|| {
            json!({
                "id": collection_slug(feature_id).unwrap_or_else(|| feature_id.to_string()),
                "name": collection_slug(feature_id)
                    .map(|slug| {
                        slug.split('-')
                            .filter(|segment| !segment.is_empty())
                            .map(|segment| {
                                let mut chars = segment.chars();
                                match chars.next() {
                                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                                    None => String::new(),
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| feature_id.to_string()),
                "version": "latest",
                "options": {}
            })
        });
        let source_information = json!({
            "type": "oci",
            "userFeatureId": feature_id,
            "userFeatureIdWithoutVersion": normalize_collection_reference(feature_id),
        });
        let installation = FeatureInstallation {
            source: FeatureInstallationSource::Published(feature_id.to_string()),
            env: feature_option_values_from_manifest(&manifest, value),
        };
        (manifest, source_information, installation)
    };

    let options = feature_options(&manifest, value);
    let metadata_entry = feature_metadata_entry(&manifest);
    let depends_on = manifest
        .get("dependsOn")
        .and_then(Value::as_object)
        .map(|entries| entries.keys().cloned().collect())
        .unwrap_or_default();

    Ok(FeatureSpec {
        manifest,
        options,
        source_information,
        metadata_entry,
        installation,
        depends_on,
    })
}

fn is_local_feature_reference(feature_id: &str) -> bool {
    feature_id.starts_with('.') || feature_id.starts_with('/') || feature_id.starts_with("file://")
}

fn resolve_local_feature_path(config_root: &Path, feature_id: &str) -> PathBuf {
    if let Some(path) = feature_id.strip_prefix("file://") {
        return PathBuf::from(path);
    }
    let path = PathBuf::from(feature_id);
    if path.is_absolute() {
        path
    } else {
        config_root.join(path)
    }
}

fn feature_object(manifest: &Value, options: &Value, value: &Value) -> Value {
    let mut feature = manifest.as_object().cloned().unwrap_or_default();
    feature.insert("options".to_string(), options.clone());
    feature.insert("value".to_string(), value.clone());
    feature.insert("included".to_string(), Value::Bool(true));
    migrate_legacy_customizations(&mut feature);
    Value::Object(feature)
}

fn migrate_legacy_customizations(feature: &mut Map<String, Value>) {
    let extensions = feature.remove("extensions");
    let settings = feature.remove("settings");
    if extensions.is_none() && settings.is_none() {
        return;
    }

    let customizations = feature
        .entry("customizations".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("customizations object");
    let vscode = customizations
        .entry("vscode".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("vscode customizations object");
    if let Some(extensions) = extensions {
        let target = vscode
            .entry("extensions".to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("extensions array");
        if let Some(values) = extensions.as_array() {
            target.extend(values.iter().cloned());
        }
    }
    if let Some(settings) = settings {
        let target = vscode
            .entry("settings".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("settings object");
        if let Some(values) = settings.as_object() {
            target.extend(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
    }
}

fn feature_options(manifest: &Value, value: &Value) -> Value {
    let mut merged = Map::new();
    if let Some(options) = manifest.get("options").and_then(Value::as_object) {
        for (key, option) in options {
            if let Some(default) = option.get("default") {
                merged.insert(key.clone(), default.clone());
            }
        }
    }
    if let Some(overrides) = value.as_object() {
        merged.extend(
            overrides
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    Value::Object(merged)
}

fn feature_metadata_entry(manifest: &Value) -> Value {
    let Some(entries) = manifest.as_object() else {
        return Value::Object(Map::new());
    };
    let mut metadata = Map::new();
    for key in [
        "containerEnv",
        "customizations",
        "entrypoint",
        "hostRequirements",
        "init",
        "mounts",
        "overrideCommand",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
        "portsAttributes",
        "otherPortsAttributes",
        "forwardPorts",
        "privileged",
        "capAdd",
        "securityOpt",
        "remoteEnv",
        "remoteUser",
        "containerUser",
        "shutdownAction",
        "updateRemoteUserUID",
        "userEnvProbe",
        "waitFor",
    ] {
        if let Some(value) = entries.get(key) {
            metadata.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(metadata)
}

fn feature_option_values_from_manifest(manifest: &Value, value: &Value) -> Vec<(String, String)> {
    let mut merged = Map::new();
    if let Some(options) = manifest.get("options").and_then(Value::as_object) {
        for (key, option) in options {
            if let Some(default) = option.get("default") {
                merged.insert(key.clone(), default.clone());
            }
        }
    }
    if let Some(overrides) = value.as_object() {
        merged.extend(
            overrides
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    merged
        .into_iter()
        .map(|(key, value)| (feature_option_env_name(&key), json_value_to_env(&value)))
        .collect()
}

fn feature_option_env_name(key: &str) -> String {
    key.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn json_value_to_env(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

pub(crate) fn materialize_feature_installation(
    installation: &FeatureInstallation,
    destination: &Path,
) -> Result<(), String> {
    match &installation.source {
        FeatureInstallationSource::Local(path) => {
            common::copy_directory_recursive(path, destination)?;
            ensure_feature_install_script(destination)
        }
        FeatureInstallationSource::Published(feature_id) => {
            let manifest = published_feature_manifest(feature_id)
                .ok_or_else(|| format!("Unknown published feature: {feature_id}"))?;
            fs::create_dir_all(destination).map_err(|error| error.to_string())?;
            fs::write(
                destination.join("devcontainer-feature.json"),
                serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            fs::write(
                destination.join("install.sh"),
                published_feature_install_script(feature_id),
            )
            .map_err(|error| error.to_string())?;
            ensure_feature_install_script(destination)
        }
    }
}

fn ensure_feature_install_script(destination: &Path) -> Result<(), String> {
    let install_path = destination.join("install.sh");
    if install_path.is_file() {
        return Ok(());
    }
    fs::write(&install_path, "#!/bin/sh\nset -eu\n").map_err(|error| error.to_string())
}

pub(crate) fn apply_feature_metadata(configuration: &Value, metadata_entries: &[Value]) -> Value {
    let mut merged = configuration.as_object().cloned().unwrap_or_default();
    for metadata in metadata_entries {
        merge_boolean_true(&mut merged, metadata, "init");
        merge_boolean_true(&mut merged, metadata, "privileged");
        merge_unique_array(&mut merged, metadata, "capAdd");
        merge_unique_array(&mut merged, metadata, "securityOpt");
        merge_unique_array(&mut merged, metadata, "mounts");
        merge_unique_array(&mut merged, metadata, "forwardPorts");
        merge_object(&mut merged, metadata, "containerEnv");
        merge_object(&mut merged, metadata, "remoteEnv");
        merge_object(&mut merged, metadata, "portsAttributes");
        merge_object(&mut merged, metadata, "customizations");
        merge_last_value(&mut merged, metadata, "containerUser");
        merge_last_value(&mut merged, metadata, "entrypoint");
        merge_last_value(&mut merged, metadata, "otherPortsAttributes");
        merge_last_value(&mut merged, metadata, "overrideCommand");
        merge_last_value(&mut merged, metadata, "remoteUser");
        merge_last_value(&mut merged, metadata, "shutdownAction");
        merge_last_value(&mut merged, metadata, "updateRemoteUserUID");
        merge_last_value(&mut merged, metadata, "userEnvProbe");
        merge_last_value(&mut merged, metadata, "waitFor");
        for key in [
            "onCreateCommand",
            "updateContentCommand",
            "postCreateCommand",
            "postStartCommand",
            "postAttachCommand",
        ] {
            merge_lifecycle_value(&mut merged, metadata, key);
        }
    }
    Value::Object(merged)
}

fn merge_boolean_true(merged: &mut Map<String, Value>, metadata: &Value, key: &str) {
    if metadata.get(key).and_then(Value::as_bool) == Some(true) {
        merged.insert(key.to_string(), Value::Bool(true));
    }
}

fn merge_unique_array(merged: &mut Map<String, Value>, metadata: &Value, key: &str) {
    let Some(values) = metadata.get(key).and_then(Value::as_array) else {
        return;
    };
    let target = merged
        .entry(key.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("array field");
    for value in values {
        if !target.iter().any(|existing| existing == value) {
            target.push(value.clone());
        }
    }
}

fn merge_object(merged: &mut Map<String, Value>, metadata: &Value, key: &str) {
    let Some(values) = metadata.get(key).and_then(Value::as_object) else {
        return;
    };
    let target = merged
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("object field");
    target.extend(
        values
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
}

fn merge_last_value(merged: &mut Map<String, Value>, metadata: &Value, key: &str) {
    if let Some(value) = metadata.get(key) {
        merged.insert(key.to_string(), value.clone());
    }
}

fn merge_lifecycle_value(merged: &mut Map<String, Value>, metadata: &Value, key: &str) {
    let Some(value) = metadata.get(key) else {
        return;
    };
    let combined = merged
        .get(key)
        .map(flatten_lifecycle_value)
        .unwrap_or_default()
        .into_iter()
        .chain(flatten_lifecycle_value(value))
        .collect::<Vec<_>>();
    match combined.len() {
        0 => {}
        1 => {
            merged.insert(
                key.to_string(),
                combined
                    .into_iter()
                    .next()
                    .expect("single lifecycle command"),
            );
        }
        _ => {
            merged.insert(
                key.to_string(),
                Value::Object(
                    combined
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| (index.to_string(), value))
                        .collect(),
                ),
            );
        }
    }
}

fn flatten_lifecycle_value(value: &Value) -> Vec<Value> {
    match value {
        Value::String(_) | Value::Array(_) => vec![value.clone()],
        Value::Object(entries) => entries.values().flat_map(flatten_lifecycle_value).collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn feature_installation_name(installation: &FeatureInstallation) -> String {
    match &installation.source {
        FeatureInstallationSource::Local(path) => path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("feature")
            .to_string(),
        FeatureInstallationSource::Published(feature_id) => {
            collection_slug(feature_id).unwrap_or_else(|| "published-feature".to_string())
        }
    }
}
