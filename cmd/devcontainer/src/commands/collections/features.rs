use std::path::Path;

use serde_json::{json, Value};

use super::registry::published_feature_manifest;
use crate::commands::common;

pub(super) fn build_features_resolve_dependencies_payload(
    args: &[String],
) -> Result<Value, String> {
    let (_, _, configuration) = common::load_resolved_config(args)?;
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut ordered = Vec::new();

    if let Some(override_order) = configuration
        .get("overrideFeatureInstallOrder")
        .and_then(Value::as_array)
    {
        for entry in override_order.iter().filter_map(Value::as_str) {
            if features.contains_key(entry) {
                ordered.push(Value::String(entry.to_string()));
            }
        }
    }

    for feature in features.keys() {
        if !ordered.iter().any(|value| value == feature) {
            ordered.push(Value::String(feature.clone()));
        }
    }

    Ok(json!({
        "outcome": "success",
        "command": "features resolve-dependencies",
        "resolvedFeatures": ordered,
    }))
}

pub(super) fn build_feature_info_payload(mode: &str, feature_path: &str) -> Result<Value, String> {
    if mode != "manifest" {
        return Err(format!("Unsupported features info mode: {mode}"));
    }

    let manifest = if feature_path.starts_with("ghcr.io/") {
        published_feature_manifest(feature_path)
            .ok_or_else(|| format!("Unknown published feature: {feature_path}"))?
    } else {
        common::parse_manifest(Path::new(feature_path), "devcontainer-feature.json")?
    };
    Ok(json!({
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "name": manifest.get("name").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "version": manifest.get("version").cloned().unwrap_or_else(|| Value::String("0.0.0".to_string())),
        "options": manifest.get("options").cloned().unwrap_or_else(|| json!({})),
    }))
}
