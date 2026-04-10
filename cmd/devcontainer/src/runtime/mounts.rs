//! Shared mount parsing and normalization helpers for runtime code.

use serde_json::{Map, Value};

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

pub(crate) fn split_mount_options(mount: &str) -> Vec<String> {
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

pub(crate) fn mount_value_to_engine_arg(value: &Value) -> Option<String> {
    match value {
        Value::String(mount) => Some(mount.clone()),
        Value::Object(entries) => mount_object_to_engine_arg(entries),
        _ => None,
    }
}

fn mount_object_to_engine_arg(entries: &Map<String, Value>) -> Option<String> {
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
            "type" | "source" | "src" | "target" | "destination" | "dst" | "readonly" | "readOnly"
        ) {
            continue;
        }
        if let Some(value) = mount_option_value(value) {
            options.push(format!("{key}={value}"));
        }
    }
    (!options.is_empty()).then(|| options.join(","))
}

fn mount_option_value(value: &Value) -> Option<String> {
    match value {
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{mount_option_target, mount_value_to_engine_arg};

    #[test]
    fn mount_option_target_reads_quoted_targets() {
        assert_eq!(
            mount_option_target(r#"type=bind,source=/tmp/src,target="/workspace,with,comma""#),
            Some("/workspace,with,comma".to_string())
        );
    }

    #[test]
    fn mount_value_to_engine_arg_preserves_read_only_and_alias_keys() {
        let mount = mount_value_to_engine_arg(&json!({
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
    fn mount_value_to_engine_arg_preserves_additional_scalar_options() {
        let mount = mount_value_to_engine_arg(&json!({
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
}
