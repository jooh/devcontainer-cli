//! Mount parsing and normalization helpers for compose override files.

use serde_json::{Map, Number, Value};

use crate::runtime::context::{
    additional_mounts_for_workspace_target, workspace_mount_for_args, ResolvedConfig,
};
use crate::runtime::mounts::split_mount_options;

pub(super) enum ComposeVolumeEntry {
    Short(String),
    Long(ComposeMountDefinition),
}

pub(super) struct ComposeMountDefinition {
    pub(super) fields: Map<String, Value>,
}

pub(super) fn compose_workspace_volume(
    resolved: &ResolvedConfig,
    args: &[String],
    remote_workspace_folder: &str,
) -> Option<ComposeVolumeEntry> {
    let mount = workspace_mount_for_args(resolved, remote_workspace_folder, args);
    let definition = compose_mount_definition_from_str(&mount)?;
    if definition.mount_type().unwrap_or("bind") != "bind" {
        return None;
    }
    definition
        .short_syntax()
        .map(ComposeVolumeEntry::Short)
        .or(Some(ComposeVolumeEntry::Long(definition)))
}

pub(super) fn compose_additional_volumes(
    resolved: &ResolvedConfig,
    args: &[String],
) -> Vec<ComposeVolumeEntry> {
    let mut volumes: Vec<ComposeVolumeEntry> = resolved
        .configuration
        .get("mounts")
        .and_then(Value::as_array)
        .map(|mounts| mounts.iter().filter_map(compose_mount_definition).collect())
        .unwrap_or_default();
    if resolved.configuration.get("workspaceMount").is_none() {
        let remote_workspace_folder =
            crate::runtime::context::remote_workspace_folder_for_args(resolved, args);
        volumes.extend(
            additional_mounts_for_workspace_target(resolved, &remote_workspace_folder, args)
                .iter()
                .filter_map(|mount| compose_mount_definition_from_str(mount))
                .map(ComposeVolumeEntry::Long),
        );
    }
    volumes
}

fn compose_mount_definition(value: &Value) -> Option<ComposeVolumeEntry> {
    match value {
        Value::String(text) => {
            compose_mount_definition_from_str(text).map(ComposeVolumeEntry::Long)
        }
        Value::Object(entries) => {
            let mut fields = Map::new();
            fields.insert(
                "type".to_string(),
                Value::String(
                    entries
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("bind")
                        .to_string(),
                ),
            );
            if let Some(source) = entries
                .get("source")
                .or_else(|| entries.get("src"))
                .and_then(Value::as_str)
            {
                fields.insert("source".to_string(), Value::String(source.to_string()));
            }
            let target = entries
                .get("target")
                .or_else(|| entries.get("destination"))
                .or_else(|| entries.get("dst"))
                .and_then(Value::as_str)?;
            fields.insert("target".to_string(), Value::String(target.to_string()));
            if entries
                .get("readonly")
                .or_else(|| entries.get("readOnly"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                fields.insert("read_only".to_string(), Value::Bool(true));
            }
            if let Some(external) = entries.get("external").and_then(Value::as_bool) {
                insert_nested_mount_value(
                    &mut fields,
                    &["volume"],
                    "external",
                    Value::Bool(external),
                );
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
                        | "external"
                ) {
                    continue;
                }
                fields.insert(key.clone(), value.clone());
            }
            Some(ComposeVolumeEntry::Long(ComposeMountDefinition { fields }))
        }
        _ => None,
    }
}

fn compose_mount_definition_from_str(mount: &str) -> Option<ComposeMountDefinition> {
    let mut fields = Map::new();
    fields.insert("type".to_string(), Value::String("bind".to_string()));
    for option in split_mount_options(mount) {
        if option == "readonly" || option == "ro" {
            fields.insert("read_only".to_string(), Value::Bool(true));
            continue;
        }
        if let Some(value) = option.strip_prefix("type=") {
            fields.insert(
                "type".to_string(),
                Value::String(value.trim_matches('"').to_string()),
            );
        } else if let Some(value) = option
            .strip_prefix("source=")
            .or_else(|| option.strip_prefix("src="))
        {
            fields.insert(
                "source".to_string(),
                Value::String(value.trim_matches('"').to_string()),
            );
        } else if let Some(value) = option
            .strip_prefix("target=")
            .or_else(|| option.strip_prefix("destination="))
            .or_else(|| option.strip_prefix("dst="))
        {
            fields.insert(
                "target".to_string(),
                Value::String(value.trim_matches('"').to_string()),
            );
        } else if let Some(value) = option.strip_prefix("external=") {
            if let Some(external) = parse_mount_option_scalar(value).as_bool() {
                insert_nested_mount_value(
                    &mut fields,
                    &["volume"],
                    "external",
                    Value::Bool(external),
                );
            }
        } else if let Some((key, value)) = option.split_once('=') {
            let path = mount_option_key_path(key);
            if let Some((leaf, parents)) = path.split_last() {
                insert_nested_mount_value(
                    &mut fields,
                    parents,
                    leaf,
                    parse_mount_option_scalar(value),
                );
            }
        }
    }

    fields
        .contains_key("target")
        .then_some(ComposeMountDefinition { fields })
}

impl ComposeMountDefinition {
    pub(super) fn mount_type(&self) -> Option<&str> {
        self.fields.get("type").and_then(Value::as_str)
    }

    pub(super) fn short_syntax(&self) -> Option<String> {
        if self.mount_type().unwrap_or("bind") != "bind" {
            return None;
        }
        if self
            .fields
            .keys()
            .any(|key| !matches!(key.as_str(), "type" | "source" | "target" | "read_only"))
        {
            return None;
        }
        let source = self.fields.get("source").and_then(Value::as_str)?;
        let target = self.fields.get("target").and_then(Value::as_str)?;
        let mut volume = format!("{source}:{target}");
        if self
            .fields
            .get("read_only")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            volume.push_str(":ro");
        }
        Some(volume)
    }
}

fn mount_option_key_path(key: &str) -> Vec<&str> {
    match key {
        "bind-propagation" => vec!["bind", "propagation"],
        "volume-nocopy" => vec!["volume", "nocopy"],
        _ => key.split('.').collect(),
    }
}

fn parse_mount_option_scalar(value: &str) -> Value {
    let value = value.trim_matches('"');
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => parse_mount_option_number(value).unwrap_or_else(|| Value::String(value.to_string())),
    }
}

fn parse_mount_option_number(value: &str) -> Option<Value> {
    if let Ok(number) = value.parse::<i64>() {
        return Some(Value::Number(number.into()));
    }
    if let Ok(number) = value.parse::<u64>() {
        return Some(Value::Number(number.into()));
    }
    value
        .parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map(Value::Number)
}

fn insert_nested_mount_value(
    fields: &mut Map<String, Value>,
    parents: &[&str],
    leaf: &str,
    value: Value,
) {
    if parents.is_empty() {
        fields.insert(leaf.to_string(), value);
        return;
    }

    let entry = fields
        .entry(parents[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    let child = entry.as_object_mut().expect("object mount option");
    insert_nested_mount_value(child, &parents[1..], leaf, value);
}

pub(super) fn compose_environment(configuration: &Value) -> Option<Vec<(String, String)>> {
    let env = configuration
        .get("containerEnv")
        .and_then(Value::as_object)?
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|text| (key.clone(), text.to_string())))
        .collect::<Vec<_>>();
    (!env.is_empty()).then_some(env)
}
