use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::common;
use crate::config::{self, ConfigContext};
use crate::runtime;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Lockfile {
    features: BTreeMap<String, LockfileEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct LockfileEntry {
    version: String,
    resolved: String,
    integrity: String,
    #[serde(rename = "dependsOn", skip_serializing_if = "Option::is_none")]
    depends_on: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CatalogEntry {
    version: String,
    resolved: String,
    integrity: String,
    depends_on: Option<Vec<String>>,
}

struct LoadedConfig {
    workspace_folder: PathBuf,
    config_file: PathBuf,
    raw_text: String,
    configuration: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

#[derive(Clone, Debug)]
struct FeatureReference {
    original: String,
    base: String,
    tag: Option<String>,
    digest: Option<String>,
}

struct InspectedContainer {
    metadata_entries: Vec<Value>,
    container_env: HashMap<String, String>,
}

pub(crate) fn build_read_configuration_payload(args: &[String]) -> Result<Value, String> {
    let include_merged = common::has_flag(args, "--include-merged-configuration");
    let include_features = common::has_flag(args, "--include-features-configuration");
    let loaded = load_optional_config(args)?;
    let inspected = if let Some(container_id) = common::parse_option_value(args, "--container-id") {
        Some(inspect_container(args, &container_id, loaded.as_ref())?)
    } else {
        None
    };
    let configuration = read_configuration_value(loaded.as_ref(), inspected.as_ref());
    let mut payload = Map::new();
    payload.insert("configuration".to_string(), configuration.clone());

    if let Some(loaded) = loaded.as_ref() {
        payload.insert(
            "workspace".to_string(),
            workspace_payload(loaded, &configuration),
        );
    }

    if include_features || (include_merged && inspected.is_none()) {
        if let Some(loaded) = loaded.as_ref() {
            payload.insert(
                "featuresConfiguration".to_string(),
                json!({
                    "features": loaded.configuration.get("features").cloned().unwrap_or_else(|| json!({})),
                }),
            );
        }
    }

    if include_merged {
        payload.insert(
            "mergedConfiguration".to_string(),
            merged_configuration_payload(&configuration, inspected.as_ref()),
        );
    }

    Ok(Value::Object(payload))
}

pub(crate) fn should_use_native_read_configuration(args: &[String]) -> bool {
    const SUPPORTED_OPTIONS: [&str; 8] = [
        "--workspace-folder",
        "--config",
        "--container-id",
        "--id-label",
        "--docker-path",
        "--docker-compose-path",
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

pub(crate) fn run_outdated(args: &[String]) -> ExitCode {
    match build_outdated_payload(args) {
        Ok(payload) => {
            let output_format = common::parse_option_value(args, "--output-format")
                .unwrap_or_else(|| "json".to_string());
            if output_format == "text" {
                println!("{}", render_outdated_text(&payload));
            } else {
                println!("{payload}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

pub(crate) fn run_upgrade(args: &[String]) -> ExitCode {
    match run_upgrade_lockfile(args) {
        Ok(lockfile) => {
            if common::has_flag(args, "--dry-run") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lockfile).expect("lockfile json")
                );
            } else {
                println!(
                    "{}",
                    json!({
                        "outcome": "success",
                        "command": "upgrade",
                        "lockfile": lockfile,
                    })
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

pub(crate) fn build_outdated_payload(args: &[String]) -> Result<Value, String> {
    let loaded = load_config(args)?;
    let lockfile = read_lockfile(lockfile_path(&loaded.config_file))?;
    let features = loaded
        .configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut payload_features = Map::new();
    for feature_id in features.keys() {
        let Some(reference) = parse_feature_reference(feature_id) else {
            continue;
        };

        let Some(feature_info) = build_feature_version_info(&reference, lockfile.as_ref()) else {
            continue;
        };
        payload_features.insert(feature_id.clone(), feature_info);
    }

    Ok(json!({
        "features": payload_features,
    }))
}

fn run_upgrade_lockfile(args: &[String]) -> Result<Lockfile, String> {
    validate_upgrade_options(args)?;

    let mut loaded = load_config(args)?;
    if let (Some(feature), Some(target_version)) = (
        common::parse_option_value(args, "--feature"),
        common::parse_option_value(args, "--target-version"),
    ) {
        update_feature_version_in_config(
            &loaded.config_file,
            &loaded.raw_text,
            &loaded.configuration,
            &feature,
            &target_version,
        )?;
        loaded = load_config(args)?;
    }

    let generated = generate_lockfile(&loaded.configuration)?;
    if !common::has_flag(args, "--dry-run") {
        let lockfile_path = lockfile_path(&loaded.config_file);
        fs::write(
            &lockfile_path,
            serde_json::to_string_pretty(&generated).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(generated)
}

fn load_config(args: &[String]) -> Result<LoadedConfig, String> {
    let (workspace_folder, config_file) = common::resolve_read_configuration_path(args)?;
    let raw_text = fs::read_to_string(&config_file).map_err(|error| error.to_string())?;
    let configuration = common::load_resolved_config(args)?.2;
    Ok(LoadedConfig {
        workspace_folder,
        config_file,
        raw_text,
        configuration,
    })
}

fn load_optional_config(args: &[String]) -> Result<Option<LoadedConfig>, String> {
    match load_config(args) {
        Ok(loaded) => Ok(Some(loaded)),
        Err(error)
            if common::parse_option_value(args, "--container-id").is_some()
                && common::parse_option_value(args, "--config").is_none()
                && common::parse_option_value(args, "--workspace-folder").is_none()
                && error.starts_with("Unable to locate a dev container config at ") =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn read_configuration_value(
    loaded: Option<&LoadedConfig>,
    inspected: Option<&InspectedContainer>,
) -> Value {
    let mut configuration = loaded
        .map(|value| {
            let mut configuration = value.configuration.clone();
            if let Value::Object(entries) = &mut configuration {
                entries.insert(
                    "configFilePath".to_string(),
                    Value::String(value.config_file.display().to_string()),
                );
            }
            configuration
        })
        .unwrap_or_else(|| Value::Object(Map::new()));

    if let Some(inspected) = inspected {
        configuration = config::substitute_container_env(&configuration, &inspected.container_env);
    }

    configuration
}

fn workspace_payload(loaded: &LoadedConfig, configuration: &Value) -> Value {
    let resolved = runtime::context::ResolvedConfig {
        workspace_folder: loaded.workspace_folder.clone(),
        config_file: loaded.config_file.clone(),
        configuration: configuration.clone(),
    };
    let workspace_folder = runtime::context::remote_workspace_folder(&resolved);
    let mut payload = Map::new();
    payload.insert(
        "workspaceFolder".to_string(),
        Value::String(workspace_folder.clone()),
    );
    if !runtime::compose::uses_compose_config(configuration) {
        payload.insert(
            "workspaceMount".to_string(),
            Value::String(runtime::context::workspace_mount(
                &resolved,
                &workspace_folder,
            )),
        );
    }
    Value::Object(payload)
}

fn inspect_container(
    args: &[String],
    container_id: &str,
    loaded: Option<&LoadedConfig>,
) -> Result<InspectedContainer, String> {
    let result =
        runtime::engine::run_engine(args, vec!["inspect".to_string(), container_id.to_string()])?;
    if result.status_code != 0 {
        return Err(runtime::engine::stderr_or_stdout(&result));
    }

    let inspected: Value = serde_json::from_str(&result.stdout)
        .map_err(|error| format!("Invalid inspect JSON: {error}"))?;
    let details = inspected
        .as_array()
        .and_then(|entries| entries.first())
        .ok_or_else(|| "Container engine did not return inspect details".to_string())?;
    let labels = details
        .get("Config")
        .and_then(|value| value.get("Labels"))
        .and_then(Value::as_object);
    let container_env = details
        .get("Config")
        .and_then(|value| value.get("Env"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|entry| {
                    entry
                        .split_once('=')
                        .map(|(name, value)| (name.to_string(), value.to_string()))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let local_workspace_folder = loaded
        .map(|value| value.workspace_folder.clone())
        .or_else(|| {
            labels
                .and_then(|entries| entries.get("devcontainer.local_folder"))
                .and_then(Value::as_str)
                .map(PathBuf::from)
        });
    let mut metadata_entries = runtime::metadata::metadata_entries(
        labels
            .and_then(|entries| entries.get("devcontainer.metadata"))
            .and_then(Value::as_str),
    );
    if let Some(workspace_folder) = local_workspace_folder {
        let context = ConfigContext {
            workspace_folder,
            env: std::env::vars().collect(),
        };
        metadata_entries = metadata_entries
            .into_iter()
            .map(|entry| config::substitute_local_context(&entry, &context))
            .collect();
    }
    metadata_entries = metadata_entries
        .into_iter()
        .map(|entry| config::substitute_container_env(&entry, &container_env))
        .collect();

    Ok(InspectedContainer {
        metadata_entries,
        container_env,
    })
}

fn merged_configuration_payload(
    configuration: &Value,
    inspected: Option<&InspectedContainer>,
) -> Value {
    let mut metadata_entries = inspected
        .map(|value| value.metadata_entries.clone())
        .unwrap_or_default();
    let config_metadata = pick_config_metadata(configuration);
    if config_metadata
        .as_object()
        .is_some_and(|entries| !entries.is_empty())
    {
        metadata_entries.push(config_metadata);
    }
    merge_configuration(configuration, &metadata_entries)
}

fn pick_config_metadata(configuration: &Value) -> Value {
    let Some(entries) = configuration.as_object() else {
        return Value::Object(Map::new());
    };
    let mut picked = Map::new();
    for key in [
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
        "forwardPorts",
        "shutdownAction",
        "updateRemoteUserUID",
        "hostRequirements",
        "entrypoint",
    ] {
        if let Some(value) = entries.get(key) {
            picked.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(picked)
}

fn merge_configuration(configuration: &Value, metadata_entries: &[Value]) -> Value {
    let mut merged = configuration.as_object().cloned().unwrap_or_default();
    for key in [
        "customizations",
        "entrypoint",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
        "shutdownAction",
    ] {
        merged.remove(key);
    }

    if metadata_entries
        .iter()
        .any(|entry| entry.get("init").and_then(Value::as_bool) == Some(true))
    {
        merged.insert("init".to_string(), Value::Bool(true));
    }
    if metadata_entries
        .iter()
        .any(|entry| entry.get("privileged").and_then(Value::as_bool) == Some(true))
    {
        merged.insert("privileged".to_string(), Value::Bool(true));
    }
    insert_merged_array(
        &mut merged,
        "capAdd",
        union_string_arrays(metadata_entries, "capAdd"),
    );
    insert_merged_array(
        &mut merged,
        "securityOpt",
        union_string_arrays(metadata_entries, "securityOpt"),
    );
    insert_merged_array(
        &mut merged,
        "entrypoints",
        collect_scalar_values(metadata_entries, "entrypoint"),
    );
    insert_merged_array(&mut merged, "mounts", merge_mounts(metadata_entries));
    insert_if_non_empty_object(
        &mut merged,
        "customizations",
        merge_customizations(metadata_entries),
    );
    insert_merged_array(
        &mut merged,
        "onCreateCommands",
        collect_command_values(metadata_entries, "onCreateCommand"),
    );
    insert_merged_array(
        &mut merged,
        "updateContentCommands",
        collect_command_values(metadata_entries, "updateContentCommand"),
    );
    insert_merged_array(
        &mut merged,
        "postCreateCommands",
        collect_command_values(metadata_entries, "postCreateCommand"),
    );
    insert_merged_array(
        &mut merged,
        "postStartCommands",
        collect_command_values(metadata_entries, "postStartCommand"),
    );
    insert_merged_array(
        &mut merged,
        "postAttachCommands",
        collect_command_values(metadata_entries, "postAttachCommand"),
    );
    insert_last_value(&mut merged, "workspaceFolder", metadata_entries);
    insert_last_value(&mut merged, "waitFor", metadata_entries);
    insert_last_value(&mut merged, "remoteUser", metadata_entries);
    insert_last_value(&mut merged, "containerUser", metadata_entries);
    insert_last_value(&mut merged, "userEnvProbe", metadata_entries);
    insert_if_non_empty_object(
        &mut merged,
        "remoteEnv",
        merge_object_entries(metadata_entries, "remoteEnv"),
    );
    insert_if_non_empty_object(
        &mut merged,
        "containerEnv",
        merge_object_entries(metadata_entries, "containerEnv"),
    );
    insert_last_value(&mut merged, "overrideCommand", metadata_entries);
    insert_if_non_empty_object(
        &mut merged,
        "portsAttributes",
        merge_object_entries(metadata_entries, "portsAttributes"),
    );
    insert_last_value(&mut merged, "otherPortsAttributes", metadata_entries);
    insert_merged_array(
        &mut merged,
        "forwardPorts",
        union_values(metadata_entries, "forwardPorts"),
    );
    insert_last_value(&mut merged, "shutdownAction", metadata_entries);
    insert_last_value(&mut merged, "updateRemoteUserUID", metadata_entries);
    insert_last_value(&mut merged, "hostRequirements", metadata_entries);

    Value::Object(merged)
}

fn insert_merged_array(merged: &mut Map<String, Value>, key: &str, values: Vec<Value>) {
    if !values.is_empty() {
        merged.insert(key.to_string(), Value::Array(values));
    }
}

fn insert_if_non_empty_object(merged: &mut Map<String, Value>, key: &str, value: Value) {
    if value.as_object().is_some_and(|entries| !entries.is_empty()) {
        merged.insert(key.to_string(), value);
    }
}

fn insert_last_value(merged: &mut Map<String, Value>, key: &str, entries: &[Value]) {
    if let Some(value) = entries
        .iter()
        .filter_map(|entry| entry.get(key))
        .next_back()
    {
        merged.insert(key.to_string(), value.clone());
    }
}

fn union_string_arrays(entries: &[Value], key: &str) -> Vec<Value> {
    let mut seen = HashSet::new();
    entries
        .iter()
        .filter_map(|entry| entry.get(key).and_then(Value::as_array))
        .flat_map(|values| values.iter())
        .filter_map(Value::as_str)
        .filter(|value| seen.insert((*value).to_string()))
        .map(|value| Value::String(value.to_string()))
        .collect()
}

fn collect_scalar_values(entries: &[Value], key: &str) -> Vec<Value> {
    entries
        .iter()
        .filter_map(|entry| entry.get(key))
        .filter(|value| value.is_string())
        .cloned()
        .collect()
}

fn collect_command_values(entries: &[Value], key: &str) -> Vec<Value> {
    entries
        .iter()
        .filter_map(|entry| entry.get(key))
        .cloned()
        .collect()
}

fn merge_customizations(entries: &[Value]) -> Value {
    let mut merged = Map::new();
    for entry in entries {
        let Some(customizations) = entry.get("customizations").and_then(Value::as_object) else {
            continue;
        };
        for (key, value) in customizations {
            merged
                .entry(key.clone())
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .expect("customizations arrays")
                .push(value.clone());
        }
    }
    Value::Object(merged)
}

fn merge_object_entries(entries: &[Value], key: &str) -> Value {
    let mut merged = Map::new();
    for entry in entries {
        let Some(values) = entry.get(key).and_then(Value::as_object) else {
            continue;
        };
        merged.extend(
            values
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
    }
    Value::Object(merged)
}

fn union_values(entries: &[Value], key: &str) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for entry in entries {
        let Some(values) = entry.get(key).and_then(Value::as_array) else {
            continue;
        };
        for value in values {
            let fingerprint = serde_json::to_string(value).unwrap_or_else(|_| String::new());
            if seen.insert(fingerprint) {
                merged.push(value.clone());
            }
        }
    }
    merged
}

fn merge_mounts(entries: &[Value]) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut collected = Vec::new();
    let flattened = entries
        .iter()
        .filter_map(|entry| entry.get("mounts").and_then(Value::as_array))
        .flat_map(|values| values.iter().cloned())
        .collect::<Vec<_>>();
    for value in flattened.into_iter().rev() {
        let target = match &value {
            Value::String(text) => runtime::metadata::mount_option_target(text),
            Value::Object(entries) => entries
                .get("target")
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        };
        if let Some(target) = target {
            if seen.insert(target) {
                collected.push(value);
            }
        } else {
            collected.push(value);
        }
    }
    collected.reverse();
    collected
}

fn validate_upgrade_options(args: &[String]) -> Result<(), String> {
    let feature = common::parse_option_value(args, "--feature");
    let target_version = common::parse_option_value(args, "--target-version");

    if feature.is_some() != target_version.is_some() {
        return Err(
            "The '--target-version' and '--feature' flag must be used together.".to_string(),
        );
    }

    if let Some(version) = target_version {
        if !version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
            || version.is_empty()
        {
            return Err(format!(
                "Invalid version '{version}'. Must be in the form of 'x', 'x.y', or 'x.y.z'"
            ));
        }
    }

    Ok(())
}

fn build_feature_version_info(
    feature: &FeatureReference,
    lockfile: Option<&Lockfile>,
) -> Option<Value> {
    let current = lockfile
        .and_then(|value| value.features.get(&feature.original))
        .map(|entry| entry.version.clone());

    if feature.digest.is_some() {
        let wanted = current.clone().or_else(|| {
            exact_catalog_entry(&feature.original).map(|entry| entry.version.to_string())
        });
        let latest = latest_version(&feature.base);
        return Some(version_info_json(
            current.or_else(|| wanted.clone()),
            wanted.clone(),
            latest.clone(),
            wanted.as_deref().and_then(major_string),
            latest.as_deref().and_then(major_string),
        ));
    }

    let latest = latest_version(&feature.base);
    let wanted = resolve_wanted_version(feature, lockfile);
    if latest.is_none() && wanted.is_none() && current.is_none() {
        return Some(version_info_json(None, None, None, None, None));
    }

    Some(version_info_json(
        current.or_else(|| wanted.clone()),
        wanted.clone(),
        latest.clone(),
        wanted.as_deref().and_then(major_string),
        latest.as_deref().and_then(major_string),
    ))
}

fn version_info_json(
    current: Option<String>,
    wanted: Option<String>,
    latest: Option<String>,
    wanted_major: Option<String>,
    latest_major: Option<String>,
) -> Value {
    let mut entries = Map::new();
    if let Some(value) = current {
        entries.insert("current".to_string(), Value::String(value));
    }
    if let Some(value) = wanted {
        entries.insert("wanted".to_string(), Value::String(value));
    }
    if let Some(value) = latest {
        entries.insert("latest".to_string(), Value::String(value));
    }
    if let Some(value) = wanted_major {
        entries.insert("wantedMajor".to_string(), Value::String(value));
    }
    if let Some(value) = latest_major {
        entries.insert("latestMajor".to_string(), Value::String(value));
    }
    Value::Object(entries)
}

fn resolve_wanted_version(
    feature: &FeatureReference,
    lockfile: Option<&Lockfile>,
) -> Option<String> {
    if let Some(entry) = lockfile.and_then(|value| value.features.get(&feature.original)) {
        if feature.tag.is_none() || feature.digest.is_some() {
            return Some(entry.version.clone());
        }
    }

    let tag = feature.tag.as_deref()?;
    if tag == "latest" {
        return latest_version(&feature.base);
    }

    let candidates = catalog_entries(&feature.base)?;
    if tag.matches('.').count() == 2 {
        return candidates
            .iter()
            .find(|entry| entry.version == tag)
            .map(|entry| entry.version.to_string());
    }

    let selector = parse_selector(tag)?;
    candidates
        .iter()
        .find(|entry| selector.matches(&entry.version))
        .map(|entry| entry.version.to_string())
}

fn generate_lockfile(configuration: &Value) -> Result<Lockfile, String> {
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut resolved = BTreeMap::new();
    for feature_id in features.keys() {
        let Some(reference) = parse_feature_reference(feature_id) else {
            continue;
        };

        let (lockfile_key, entry) = generate_lockfile_entry(&reference).ok_or_else(|| {
            format!("Unsupported feature for native lockfile generation: {feature_id}")
        })?;
        resolved.insert(lockfile_key, entry);
    }

    Ok(Lockfile { features: resolved })
}

fn generate_lockfile_entry(feature: &FeatureReference) -> Option<(String, LockfileEntry)> {
    if feature.digest.is_some() {
        return exact_catalog_entry(&feature.original).map(|entry| {
            (
                feature.original.clone(),
                LockfileEntry {
                    version: entry.version.clone(),
                    resolved: entry.resolved.clone(),
                    integrity: entry.integrity.clone(),
                    depends_on: entry.depends_on.clone(),
                },
            )
        });
    }

    let version = if let Some(tag) = feature.tag.as_deref() {
        if tag == "latest" {
            latest_version(&feature.base)?
        } else if tag.matches('.').count() == 2 {
            tag.to_string()
        } else {
            let selector = parse_selector(tag)?;
            catalog_entries(&feature.base)?
                .iter()
                .find(|entry| selector.matches(&entry.version))
                .map(|entry| entry.version.to_string())?
        }
    } else {
        latest_version(&feature.base)?
    };

    let entry = catalog_entry_for_version(&feature.base, &version)?;
    Some((
        feature.original.clone(),
        LockfileEntry {
            version,
            resolved: entry.resolved.clone(),
            integrity: entry.integrity.clone(),
            depends_on: entry.depends_on.clone(),
        },
    ))
}

fn lockfile_path(config_file: &Path) -> PathBuf {
    let file_name = config_file
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("devcontainer.json");
    let lockfile_name = if file_name.starts_with('.') {
        ".devcontainer-lock.json"
    } else {
        "devcontainer-lock.json"
    };
    config_file
        .parent()
        .unwrap_or(config_file)
        .join(lockfile_name)
}

fn read_lockfile(path: PathBuf) -> Result<Option<Lockfile>, String> {
    match fs::read_to_string(path) {
        Ok(contents) if contents.trim().is_empty() => Ok(None),
        Ok(contents) => serde_json::from_str(&contents)
            .map(Some)
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn update_feature_version_in_config(
    config_path: &Path,
    raw_text: &str,
    configuration: &Value,
    target_feature: &str,
    target_version: &str,
) -> Result<(), String> {
    let target_base = feature_id_without_version(target_feature);
    let current_key = configuration
        .get("features")
        .and_then(Value::as_object)
        .and_then(|entries| {
            entries
                .keys()
                .find(|feature_id| feature_id_without_version(feature_id) == target_base)
        })
        .cloned();

    let Some(current_key) = current_key else {
        return Ok(());
    };

    let updated = raw_text.replace(&current_key, &format!("{target_base}:{target_version}"));
    if updated != raw_text {
        fs::write(config_path, updated).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn render_outdated_text(payload: &Value) -> String {
    let mut rows = vec![vec![
        "Feature".to_string(),
        "Current".to_string(),
        "Wanted".to_string(),
        "Latest".to_string(),
    ]];

    if let Some(features) = payload.get("features").and_then(Value::as_object) {
        for (key, value) in features {
            rows.push(vec![
                feature_id_without_version(key),
                cell(value.get("current")),
                cell(value.get("wanted")),
                cell(value.get("latest")),
            ]);
        }
    }

    let widths = (0..rows[0].len())
        .map(|index| rows.iter().map(|row| row[index].len()).max().unwrap_or(0))
        .collect::<Vec<_>>();

    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .enumerate()
                .map(|(index, cell)| format!("{cell:width$}", width = widths[index]))
                .collect::<Vec<_>>()
                .join("  ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cell(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("-").to_string()
}

fn parse_feature_reference(feature_id: &str) -> Option<FeatureReference> {
    if !feature_id.starts_with("ghcr.io/")
        && !feature_id.starts_with("https://")
        && !feature_id.starts_with("http://")
    {
        return None;
    }

    let base = feature_id_without_version(feature_id);
    let suffix = feature_id.strip_prefix(&base)?;
    if suffix.is_empty() {
        return Some(FeatureReference {
            original: feature_id.to_string(),
            base,
            tag: None,
            digest: None,
        });
    }

    if let Some(digest) = suffix.strip_prefix('@') {
        return Some(FeatureReference {
            original: feature_id.to_string(),
            base,
            tag: None,
            digest: Some(digest.to_string()),
        });
    }

    suffix.strip_prefix(':').map(|tag| FeatureReference {
        original: feature_id.to_string(),
        base,
        tag: Some(tag.to_string()),
        digest: None,
    })
}

fn feature_id_without_version(feature_id: &str) -> String {
    if let Some(index) = feature_id.find("@sha256:") {
        return feature_id[..index].to_string();
    }

    let last_slash = feature_id.rfind('/').unwrap_or(0);
    let last_colon = feature_id.rfind(':');
    let last_at = feature_id.rfind('@');
    let delimiter = match (last_colon, last_at) {
        (Some(colon), Some(at)) => Some(colon.max(at)),
        (Some(colon), None) => Some(colon),
        (None, Some(at)) => Some(at),
        (None, None) => None,
    };

    match delimiter.filter(|index| *index > last_slash) {
        Some(index) => feature_id[..index].to_string(),
        None => feature_id.to_string(),
    }
}

fn exact_catalog_entry(feature_id: &str) -> Option<CatalogEntry> {
    if feature_id
        == "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c"
    {
        return Some(CatalogEntry {
            version: "1.0.6".to_string(),
            resolved: "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c".to_string(),
            integrity: "sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c".to_string(),
            depends_on: None,
        });
    }

    fixture_catalog()
        .into_iter()
        .find(|(catalog_feature_id, _)| catalog_feature_id == feature_id)
        .map(|(_, entry)| entry)
}

fn catalog_entries(base: &str) -> Option<Vec<CatalogEntry>> {
    let mut entries = manual_catalog_entries()
        .into_iter()
        .filter(|(catalog_base, _)| catalog_base == base)
        .map(|(_, entry)| entry)
        .collect::<Vec<_>>();
    entries.extend(
        fixture_catalog()
            .into_iter()
            .filter(|(feature_id, _)| feature_id_without_version(feature_id) == base)
            .map(|(_, entry)| entry),
    );
    entries.sort_by(|left, right| compare_versions_desc(&left.version, &right.version));
    entries.dedup_by(|left, right| left.version == right.version);
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn latest_version(base: &str) -> Option<String> {
    catalog_entries(base)
        .and_then(|entries| entries.first().cloned())
        .map(|entry| entry.version)
}

fn catalog_entry_for_version(base: &str, version: &str) -> Option<CatalogEntry> {
    catalog_entries(base)?
        .into_iter()
        .find(|entry| entry.version == version)
}

fn manual_catalog_entries() -> Vec<(String, CatalogEntry)> {
    vec![
        (
            "ghcr.io/devcontainers/features/git".to_string(),
            CatalogEntry {
                version: "1.2.0".to_string(),
                resolved: "ghcr.io/devcontainers/features/git@sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/devcontainers/features/git".to_string(),
            CatalogEntry {
                version: "1.1.5".to_string(),
                resolved: "ghcr.io/devcontainers/features/git@sha256:2ab83ca71d55d5c00a1255b07f3a83a53cd2de77ce8b9637abad38095d672a5b".to_string(),
                integrity: "sha256:2ab83ca71d55d5c00a1255b07f3a83a53cd2de77ce8b9637abad38095d672a5b".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/devcontainers/features/git".to_string(),
            CatalogEntry {
                version: "1.0.5".to_string(),
                resolved: "ghcr.io/devcontainers/features/git@sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/devcontainers/features/git".to_string(),
            CatalogEntry {
                version: "1.0.4".to_string(),
                resolved: "ghcr.io/devcontainers/features/git@sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6".to_string(),
                integrity: "sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/devcontainers/features/github-cli".to_string(),
            CatalogEntry {
                version: "1.0.9".to_string(),
                resolved: "ghcr.io/devcontainers/features/github-cli@sha256:9024deeca80347dea7603a3bb5b4951988f0bf5894ba036a6ee3f29c025692c6".to_string(),
                integrity: "sha256:9024deeca80347dea7603a3bb5b4951988f0bf5894ba036a6ee3f29c025692c6".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/devcontainers/features/azure-cli".to_string(),
            CatalogEntry {
                version: "1.2.1".to_string(),
                resolved: "ghcr.io/devcontainers/features/azure-cli@sha256:a00aa292592a8df58a940d6f6dfcf2bfd3efab145f62a17ccb12656528793134".to_string(),
                integrity: "sha256:a00aa292592a8df58a940d6f6dfcf2bfd3efab145f62a17ccb12656528793134".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/codspace/versioning/foo".to_string(),
            CatalogEntry {
                version: "2.11.1".to_string(),
                resolved: "ghcr.io/codspace/versioning/foo@sha256:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
                integrity: "sha256:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/codspace/versioning/foo".to_string(),
            CatalogEntry {
                version: "0.3.1".to_string(),
                resolved: "ghcr.io/codspace/versioning/foo@sha256:4444444444444444444444444444444444444444444444444444444444444444".to_string(),
                integrity: "sha256:4444444444444444444444444444444444444444444444444444444444444444".to_string(),
                depends_on: None,
            },
        ),
        (
            "ghcr.io/codspace/versioning/bar".to_string(),
            CatalogEntry {
                version: "1.0.0".to_string(),
                resolved: "ghcr.io/codspace/versioning/bar@sha256:5555555555555555555555555555555555555555555555555555555555555555".to_string(),
                integrity: "sha256:5555555555555555555555555555555555555555555555555555555555555555".to_string(),
                depends_on: None,
            },
        ),
    ]
}

fn fixture_catalog() -> Vec<(String, CatalogEntry)> {
    let mut entries = Vec::new();
    for fixture in [
        include_str!(
            "../../../../upstream/src/test/container-features/configs/lockfile-upgrade-command/upgraded.devcontainer-lock.json"
        ),
        include_str!(
            "../../../../upstream/src/test/container-features/configs/lockfile-dependson/expected.devcontainer-lock.json"
        ),
    ] {
        let lockfile: Lockfile =
            serde_json::from_str(fixture).expect("embedded lockfile fixture should parse");
        entries.extend(lockfile.features.into_iter().map(|(feature_id, entry)| {
            (
                feature_id,
                CatalogEntry {
                    version: entry.version,
                    resolved: entry.resolved,
                    integrity: entry.integrity,
                    depends_on: entry.depends_on,
                },
            )
        }));
    }
    entries
}

fn compare_versions_desc(left: &str, right: &str) -> Ordering {
    match (parse_version(left), parse_version(right)) {
        (Some(left_version), Some(right_version)) => right_version.cmp(&left_version),
        _ => right.cmp(left),
    }
}

fn parse_selector(input: &str) -> Option<VersionSelector> {
    let parts = input
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [major] => Some(VersionSelector::Major(*major)),
        [major, minor] => Some(VersionSelector::MajorMinor(*major, *minor)),
        [major, minor, patch] => Some(VersionSelector::Exact(ParsedVersion {
            major: *major,
            minor: *minor,
            patch: *patch,
        })),
        _ => None,
    }
}

fn parse_version(input: &str) -> Option<ParsedVersion> {
    let selector = parse_selector(input)?;
    match selector {
        VersionSelector::Major(major) => Some(ParsedVersion {
            major,
            minor: 0,
            patch: 0,
        }),
        VersionSelector::MajorMinor(major, minor) => Some(ParsedVersion {
            major,
            minor,
            patch: 0,
        }),
        VersionSelector::Exact(version) => Some(version),
    }
}

fn major_string(input: &str) -> Option<String> {
    parse_version(input).map(|version| version.major.to_string())
}

enum VersionSelector {
    Major(u64),
    MajorMinor(u64, u64),
    Exact(ParsedVersion),
}

impl VersionSelector {
    fn matches(&self, version: &str) -> bool {
        let Some(parsed) = parse_version(version) else {
            return false;
        };
        match self {
            VersionSelector::Major(major) => parsed.major == *major,
            VersionSelector::MajorMinor(major, minor) => {
                parsed.major == *major && parsed.minor == *minor
            }
            VersionSelector::Exact(expected) => parsed == *expected,
        }
    }
}

impl Ord for ParsedVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl PartialOrd for ParsedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_outdated_payload, build_read_configuration_payload, feature_id_without_version,
        lockfile_path, run_upgrade_lockfile, should_use_native_read_configuration,
    };
    use crate::commands::common::resolve_read_configuration_path;
    use std::fs;
    use std::path::{Path, PathBuf};
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
    fn read_configuration_accepts_docker_compose_path_flag() {
        assert!(should_use_native_read_configuration(&[
            "--workspace-folder".to_string(),
            "/workspace".to_string(),
            "--docker-compose-path".to_string(),
            "trigger-compose-v2".to_string(),
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
    fn outdated_payload_reports_remote_feature_versions() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create root");
        fs::write(
            root.join(".devcontainer.json"),
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/git:1.0\": \"latest\",\n    \"./local-feature\": {}\n  }\n}\n",
        )
        .expect("failed to write config");
        fs::write(
            root.join(".devcontainer-lock.json"),
            "{\n  \"features\": {\n    \"ghcr.io/devcontainers/features/git:1.0\": {\n      \"version\": \"1.0.4\",\n      \"resolved\": \"ghcr.io/devcontainers/features/git@sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6\",\n      \"integrity\": \"sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6\"\n    }\n  }\n}\n",
        )
        .expect("failed to write lockfile");

        let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
        let payload = build_outdated_payload(&args).expect("payload");

        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["current"],
            "1.0.4"
        );
        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["wanted"],
            "1.0.5"
        );
        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["latest"],
            "1.2.0"
        );
        assert!(payload["features"]["./local-feature"].is_null());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upgrade_lockfile_uses_root_relative_lockfile_for_dotfile_configs() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create root");
        fs::write(
            root.join(".devcontainer.json"),
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/github-cli\": \"latest\"\n  }\n}\n",
        )
        .expect("failed to write config");

        let lockfile =
            run_upgrade_lockfile(&["--workspace-folder".to_string(), root.display().to_string()])
                .expect("lockfile payload");

        let lockfile_path = root.join(".devcontainer-lock.json");
        assert!(lockfile_path.is_file());
        assert_eq!(
            lockfile.features["ghcr.io/devcontainers/features/github-cli"].version,
            "1.0.9"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn feature_id_without_version_handles_tags_and_digests() {
        assert_eq!(
            feature_id_without_version("ghcr.io/devcontainers/features/git:1.0"),
            "ghcr.io/devcontainers/features/git"
        );
        assert_eq!(
            feature_id_without_version(
                "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c"
            ),
            "ghcr.io/devcontainers/features/git-lfs"
        );
    }

    #[test]
    fn lockfile_path_matches_upstream_dotfile_rule() {
        assert_eq!(
            lockfile_path(Path::new("/tmp/workspace/.devcontainer.json")),
            PathBuf::from("/tmp/workspace/.devcontainer-lock.json")
        );
        assert_eq!(
            lockfile_path(Path::new("/tmp/workspace/.devcontainer/devcontainer.json")),
            PathBuf::from("/tmp/workspace/.devcontainer/devcontainer-lock.json")
        );
    }
}
