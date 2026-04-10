//! Feature metadata extraction and metadata-merge policy helpers.

use serde_json::{Map, Value};

use crate::config::{flatten_lifecycle_value, lifecycle_value_from_flattened};
use crate::runtime::mounts::mount_option_target;

pub(super) fn feature_metadata_entry(manifest: &Value) -> Value {
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

pub(crate) fn apply_feature_metadata(
    configuration: &Value,
    metadata_entries: &[Value],
    skip_feature_customizations: bool,
) -> Value {
    let mut merged = configuration.as_object().cloned().unwrap_or_default();
    for metadata in metadata_entries {
        merge_boolean_true(&mut merged, metadata, "init");
        merge_boolean_true(&mut merged, metadata, "privileged");
        merge_unique_array(&mut merged, metadata, "capAdd");
        merge_unique_array(&mut merged, metadata, "securityOpt");
        merge_mounts(&mut merged, metadata);
        merge_unique_array(&mut merged, metadata, "forwardPorts");
        merge_object(&mut merged, metadata, "containerEnv");
        merge_object(&mut merged, metadata, "remoteEnv");
        merge_object(&mut merged, metadata, "portsAttributes");
        if !skip_feature_customizations {
            merge_object(&mut merged, metadata, "customizations");
        }
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

fn merge_mounts(merged: &mut Map<String, Value>, metadata: &Value) {
    let Some(values) = metadata.get("mounts").and_then(Value::as_array) else {
        return;
    };
    let target = merged
        .entry("mounts".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("array field");
    for value in values {
        if let Some(target_path) = mount_target(value) {
            if let Some(index) = target.iter().position(|existing| {
                mount_target(existing).as_deref() == Some(target_path.as_str())
            }) {
                target.remove(index);
            }
            target.push(value.clone());
            continue;
        }
        if !target.iter().any(|existing| existing == value) {
            target.push(value.clone());
        }
    }
}

fn mount_target(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => mount_option_target(text),
        Value::Object(entries) => entries
            .get("target")
            .or_else(|| entries.get("destination"))
            .or_else(|| entries.get("dst"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
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
    if let Some(value) = lifecycle_value_from_flattened(combined) {
        merged.insert(key.to_string(), value);
    }
}
