use serde_json::{json, Map, Value};

use super::inspect::{merged_configuration_payload, read_configuration_value, workspace_payload};
use super::load::load_optional_config;
use crate::commands::common;

pub(super) fn build_read_configuration_payload(args: &[String]) -> Result<Value, String> {
    let include_merged = common::has_flag(args, "--include-merged-configuration");
    let include_features = common::has_flag(args, "--include-features-configuration");
    let loaded = load_optional_config(args)?;
    let inspected = if let Some(container_id) = common::parse_option_value(args, "--container-id") {
        Some(super::inspect::inspect_container(
            args,
            &container_id,
            loaded.as_ref(),
        )?)
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

pub(super) fn should_use_native_read_configuration(args: &[String]) -> bool {
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
