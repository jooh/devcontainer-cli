//! Feature declaration parsing, dependency ordering, and source resolution helpers.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::commands::collections::registry::{
    collection_slug, normalize_collection_reference, published_feature_manifest,
};
use crate::commands::common;

use super::control::{ensure_no_disallowed_features, feature_advisories_for_oci_features};
use super::metadata::feature_metadata_entry;
use super::options::{feature_object, feature_option_values_from_manifest, feature_options};
use super::types::{
    FeatureInstallation, FeatureInstallationSource, FeatureSpec, ResolvedFeatureSupport,
};

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
    ensure_no_disallowed_features(args, &declared)?;

    let config_root = config_file.parent().unwrap_or(workspace_folder);
    let ordered_ids = resolve_feature_install_order(&declared, configuration, config_root)?;
    let mut feature_sets = Vec::new();
    let mut advisory_inputs = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut installations = Vec::new();

    for feature_id in &ordered_ids {
        let feature_value = declared
            .get(feature_id)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        let spec = resolve_feature_spec(feature_id, &feature_value, config_root)?;
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
        if matches!(
            &spec.installation.source,
            FeatureInstallationSource::Published(_)
        ) {
            if let Some(version) = spec.manifest.get("version").and_then(Value::as_str) {
                advisory_inputs.push((
                    normalize_collection_reference(feature_id),
                    version.to_string(),
                ));
            }
        }
        installations.push(spec.installation);
    }
    let feature_advisories = feature_advisories_for_oci_features(args, &advisory_inputs)?;

    Ok(Some(ResolvedFeatureSupport {
        features_configuration: json!({
            "featureSets": feature_sets,
        }),
        feature_advisories,
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
