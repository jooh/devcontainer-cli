use std::path::Path;

use serde_json::{json, Value};

use super::registry::published_feature_manifest;
use crate::commands::common;

pub(super) fn build_features_resolve_dependencies_payload(
    args: &[String],
) -> Result<Value, String> {
    let (workspace_folder, config_file, configuration) = common::load_resolved_config(args)?;
    let ordered = crate::commands::configuration::resolve_feature_support(
        args,
        &workspace_folder,
        &config_file,
        &configuration,
    )?
    .map(|resolved| {
        resolved
            .ordered_feature_ids
            .into_iter()
            .map(Value::String)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();

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
