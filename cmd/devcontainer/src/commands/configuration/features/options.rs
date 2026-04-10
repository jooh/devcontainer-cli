//! Feature option merging and feature-object shaping helpers.

use serde_json::{Map, Value};

pub(super) fn feature_object(manifest: &Value, options: &Value, value: &Value) -> Value {
    let mut feature = manifest.as_object().cloned().unwrap_or_default();
    feature.insert("options".to_string(), options.clone());
    feature.insert("value".to_string(), value.clone());
    feature.insert("included".to_string(), Value::Bool(true));
    migrate_legacy_customizations(&mut feature);
    Value::Object(feature)
}

pub(super) fn feature_options(manifest: &Value, value: &Value) -> Value {
    Value::Object(merged_feature_options(manifest, value))
}

pub(super) fn feature_option_values_from_manifest(
    manifest: &Value,
    value: &Value,
) -> Vec<(String, String)> {
    merged_feature_options(manifest, value)
        .into_iter()
        .map(|(key, value)| (feature_option_env_name(&key), json_value_to_env(&value)))
        .collect()
}

fn merged_feature_options(manifest: &Value, value: &Value) -> Map<String, Value> {
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
