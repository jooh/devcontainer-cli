use serde_json::{Map, Value};

pub(crate) fn merged_container_metadata(metadata_label: Option<&str>) -> Value {
    let Some(metadata_label) = metadata_label else {
        return Value::Object(Map::new());
    };
    let parsed = serde_json::from_str::<Value>(metadata_label).ok();
    let entries = match parsed {
        Some(Value::Array(values)) => values,
        Some(Value::Object(entries)) => vec![Value::Object(entries)],
        _ => Vec::new(),
    };

    let mut merged = Map::new();
    merge_last_metadata_value(&entries, &mut merged, "waitFor");
    merge_last_metadata_value(&entries, &mut merged, "workspaceFolder");
    merge_last_metadata_value(&entries, &mut merged, "remoteUser");
    merge_last_metadata_value(&entries, &mut merged, "containerUser");
    merge_object_metadata_value(&entries, &mut merged, "remoteEnv");
    merge_object_metadata_value(&entries, &mut merged, "containerEnv");

    for key in [
        "initializeCommand",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ] {
        if let Some(command_group) = merged_metadata_lifecycle_values(&entries, key) {
            merged.insert(key.to_string(), command_group);
        }
    }

    Value::Object(merged)
}

pub(crate) fn serialized_container_metadata(
    configuration: &Value,
    remote_workspace_folder: &str,
) -> Result<String, String> {
    let mut metadata = Map::new();
    for key in [
        "waitFor",
        "workspaceFolder",
        "remoteUser",
        "containerUser",
        "remoteEnv",
        "containerEnv",
        "initializeCommand",
        "onCreateCommand",
        "updateContentCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ] {
        if let Some(value) = configuration.get(key) {
            metadata.insert(key.to_string(), value.clone());
        }
    }
    metadata
        .entry("workspaceFolder".to_string())
        .or_insert_with(|| Value::String(remote_workspace_folder.to_string()));
    serde_json::to_string(&Value::Object(metadata))
        .map_err(|error| format!("Failed to serialize container metadata: {error}"))
}

pub(crate) fn mount_option_target(mount: &str) -> Option<String> {
    split_mount_options(mount).into_iter().find_map(|option| {
        for key in ["target", "destination", "dst"] {
            if let Some(value) = option.strip_prefix(&format!("{key}=")) {
                return Some(value.trim_matches('"').to_string());
            }
        }
        None
    })
}

fn merge_last_metadata_value(entries: &[Value], merged: &mut Map<String, Value>, key: &str) {
    if let Some(value) = entries
        .iter()
        .filter_map(|entry| entry.get(key))
        .next_back()
    {
        merged.insert(key.to_string(), value.clone());
    }
}

fn merge_object_metadata_value(entries: &[Value], merged: &mut Map<String, Value>, key: &str) {
    let combined = entries
        .iter()
        .filter_map(|entry| entry.get(key).and_then(Value::as_object))
        .fold(Map::new(), |mut combined, value| {
            combined.extend(
                value
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone())),
            );
            combined
        });
    if !combined.is_empty() {
        merged.insert(key.to_string(), Value::Object(combined));
    }
}

fn merged_metadata_lifecycle_values(entries: &[Value], key: &str) -> Option<Value> {
    let commands = entries
        .iter()
        .filter_map(|entry| entry.get(key))
        .flat_map(flatten_lifecycle_values)
        .collect::<Vec<_>>();
    match commands.len() {
        0 => None,
        1 => commands.into_iter().next(),
        _ => Some(Value::Object(
            commands
                .into_iter()
                .enumerate()
                .map(|(index, value)| (index.to_string(), value))
                .collect(),
        )),
    }
}

fn flatten_lifecycle_values(value: &Value) -> Vec<Value> {
    match value {
        Value::String(_) | Value::Array(_) => vec![value.clone()],
        Value::Object(entries) => entries
            .values()
            .flat_map(flatten_lifecycle_values)
            .collect(),
        _ => Vec::new(),
    }
}

fn split_mount_options(mount: &str) -> Vec<String> {
    let mut options = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for character in mount.chars() {
        match character {
            '"' => {
                in_quotes = !in_quotes;
                current.push(character);
            }
            ',' if !in_quotes => {
                options.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        options.push(current.trim().to_string());
    }
    options
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{merged_container_metadata, mount_option_target};

    #[test]
    fn merged_container_metadata_prefers_last_scalar_and_merges_objects() {
        let metadata = serde_json::to_string(&json!([
            {
                "workspaceFolder": "/first",
                "remoteEnv": {
                    "A": "1"
                }
            },
            {
                "workspaceFolder": "/second",
                "remoteEnv": {
                    "B": "2"
                }
            }
        ]))
        .expect("metadata json");

        let merged = merged_container_metadata(Some(&metadata));

        assert_eq!(merged["workspaceFolder"], "/second");
        assert_eq!(merged["remoteEnv"]["A"], "1");
        assert_eq!(merged["remoteEnv"]["B"], "2");
    }

    #[test]
    fn merged_container_metadata_flattens_multiple_lifecycle_entries() {
        let metadata = serde_json::to_string(&json!([
            {
                "postCreateCommand": "echo first"
            },
            {
                "postCreateCommand": {
                    "alpha": "echo second",
                    "beta": ["printf", "%s", "third"]
                }
            }
        ]))
        .expect("metadata json");

        let merged = merged_container_metadata(Some(&metadata));
        let commands = merged["postCreateCommand"]
            .as_object()
            .expect("expected flattened object");
        assert_eq!(commands.len(), 3);
    }

    #[test]
    fn mount_option_target_reads_quoted_targets() {
        assert_eq!(
            mount_option_target(r#"type=bind,source=/tmp/src,target="/workspace,with,comma""#),
            Some("/workspace,with,comma".to_string())
        );
    }
}
