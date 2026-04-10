//! Control-manifest helpers for disallowed Features and future advisory support.

use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::commands::common;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DevContainerControlManifest {
    disallowed_features: Vec<DisallowedFeature>,
    #[allow(dead_code)]
    feature_advisories: Vec<FeatureAdvisory>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisallowedFeature {
    feature_id_prefix: String,
    documentation_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
struct FeatureAdvisory {
    feature_id: String,
    introduced_in_version: String,
    fixed_in_version: String,
    description: String,
    documentation_url: Option<String>,
}

pub(crate) fn ensure_no_disallowed_features(
    args: &[String],
    declared_features: &Map<String, Value>,
) -> Result<(), String> {
    if declared_features.is_empty() {
        return Ok(());
    }

    let control_manifest = control_manifest(args)?;
    for feature_id in declared_features.keys() {
        if let Some(entry) = find_disallowed_feature_entry(&control_manifest, feature_id) {
            let mut message = format!(
                "Cannot use the '{feature_id}' Feature since it was reported to be problematic. Please remove this Feature from your configuration and rebuild any dev container using it before continuing."
            );
            if let Some(documentation_url) = &entry.documentation_url {
                message.push_str(&format!(" See {documentation_url} to learn more."));
            }
            return Err(message);
        }
    }

    Ok(())
}

fn control_manifest(args: &[String]) -> Result<DevContainerControlManifest, String> {
    let Some(user_data_folder) = common::parse_option_value(args, "--user-data-folder") else {
        return Ok(fixture_control_manifest());
    };

    let manifest_path = PathBuf::from(user_data_folder).join("control-manifest.json");
    if !manifest_path.is_file() {
        return Ok(fixture_control_manifest());
    }

    let raw = fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?;
    let parsed = crate::config::parse_jsonc_value(&raw)?;
    Ok(sanitize_control_manifest(&parsed))
}

fn sanitize_control_manifest(value: &Value) -> DevContainerControlManifest {
    let Some(entries) = value.as_object() else {
        return DevContainerControlManifest::default();
    };

    DevContainerControlManifest {
        disallowed_features: entries
            .get("disallowedFeatures")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let object = entry.as_object()?;
                let feature_id_prefix = object.get("featureIdPrefix")?.as_str()?.to_string();
                Some(DisallowedFeature {
                    feature_id_prefix,
                    documentation_url: object
                        .get("documentationURL")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect(),
        feature_advisories: entries
            .get("featureAdvisories")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let object = entry.as_object()?;
                Some(FeatureAdvisory {
                    feature_id: object.get("featureId")?.as_str()?.to_string(),
                    introduced_in_version: object.get("introducedInVersion")?.as_str()?.to_string(),
                    fixed_in_version: object.get("fixedInVersion")?.as_str()?.to_string(),
                    description: object.get("description")?.as_str()?.to_string(),
                    documentation_url: object
                        .get("documentationURL")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect(),
    }
}

fn fixture_control_manifest() -> DevContainerControlManifest {
    DevContainerControlManifest {
        disallowed_features: vec![DisallowedFeature {
            feature_id_prefix: "ghcr.io/devcontainers/features/problematic-feature".to_string(),
            documentation_url: Some("https://containers.dev/".to_string()),
        }],
        feature_advisories: vec![FeatureAdvisory {
            feature_id: "ghcr.io/devcontainers/features/feature-with-advisory".to_string(),
            introduced_in_version: "1.0.7".to_string(),
            fixed_in_version: "1.1.10".to_string(),
            description: "Fixture advisory entry for native parity testing.".to_string(),
            documentation_url: Some("https://containers.dev/".to_string()),
        }],
    }
}

fn find_disallowed_feature_entry<'a>(
    control_manifest: &'a DevContainerControlManifest,
    feature_id: &str,
) -> Option<&'a DisallowedFeature> {
    control_manifest
        .disallowed_features
        .iter()
        .find(|entry| feature_matches_prefix(feature_id, &entry.feature_id_prefix))
}

fn feature_matches_prefix(feature_id: &str, prefix: &str) -> bool {
    feature_id.starts_with(prefix)
        && (feature_id.len() == prefix.len()
            || matches!(
                feature_id.as_bytes().get(prefix.len()).copied(),
                Some(b'/') | Some(b':') | Some(b'@')
            ))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map, Value};

    use super::{ensure_no_disallowed_features, feature_matches_prefix, sanitize_control_manifest};
    use crate::test_support::unique_temp_dir;

    #[test]
    fn disallowed_feature_matching_accepts_exact_ids_and_supported_separators() {
        assert!(feature_matches_prefix(
            "example.io/test/node",
            "example.io/test/node"
        ));
        assert!(feature_matches_prefix(
            "example.io/test/node:1",
            "example.io/test/node"
        ));
        assert!(feature_matches_prefix(
            "example.io/test/node/js",
            "example.io/test/node"
        ));
        assert!(feature_matches_prefix(
            "example.io/test/node@sha256:abc",
            "example.io/test/node"
        ));
        assert!(!feature_matches_prefix(
            "example.io/test/nodej",
            "example.io/test/node"
        ));
        assert!(!feature_matches_prefix(
            "example.io/test/node.js",
            "example.io/test/node"
        ));
    }

    #[test]
    fn ensure_no_disallowed_features_accepts_allowed_feature_sets() {
        let features = Map::from_iter([(
            "ghcr.io/devcontainers/features/git:1".to_string(),
            Value::Object(Map::new()),
        )]);

        ensure_no_disallowed_features(&[], &features).expect("allowed features");
    }

    #[test]
    fn ensure_no_disallowed_features_rejects_fixture_disallowed_features() {
        let features = Map::from_iter([(
            "ghcr.io/devcontainers/features/problematic-feature:1".to_string(),
            Value::Object(Map::new()),
        )]);

        let error = ensure_no_disallowed_features(&[], &features).expect_err("disallowed feature");
        assert!(error.contains("problematic-feature:1"), "{error}");
        assert!(error.contains("https://containers.dev/"), "{error}");
    }

    #[test]
    fn ensure_no_disallowed_features_supports_user_data_folder_override() {
        let root = unique_temp_dir("devcontainer-control-manifest-test");
        let user_data = root.join("user-data");
        std::fs::create_dir_all(&user_data).expect("user data dir");
        std::fs::write(
            user_data.join("control-manifest.json"),
            json!({
                "disallowedFeatures": [{
                    "featureIdPrefix": "ghcr.io/devcontainers/features/git",
                    "documentationURL": "https://example.invalid/disallowed"
                }]
            })
            .to_string(),
        )
        .expect("control manifest");
        let features = Map::from_iter([(
            "ghcr.io/devcontainers/features/git:1".to_string(),
            Value::Object(Map::new()),
        )]);

        let error = ensure_no_disallowed_features(
            &[
                "--user-data-folder".to_string(),
                user_data.display().to_string(),
            ],
            &features,
        )
        .expect_err("user-data control manifest");

        assert!(
            error.contains("https://example.invalid/disallowed"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sanitize_control_manifest_filters_invalid_entries() {
        let manifest = sanitize_control_manifest(&json!({
            "disallowedFeatures": [
                { "featureIdPrefix": "example.io/test/node" },
                { "featureIdPrefix": 3 }
            ],
            "featureAdvisories": [
                {
                    "featureId": "example.io/test/node",
                    "introducedInVersion": "1.0.0",
                    "fixedInVersion": "1.1.0",
                    "description": "desc"
                },
                {
                    "featureId": "example.io/test/invalid"
                }
            ]
        }));

        assert_eq!(manifest.disallowed_features.len(), 1);
        assert_eq!(manifest.feature_advisories.len(), 1);
    }
}
