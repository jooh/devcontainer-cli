use std::fs;

use super::support::unique_temp_dir;
use crate::commands::collections::features::{
    build_feature_info_payload, build_features_resolve_dependencies_payload,
};

#[test]
fn feature_dependency_resolution_respects_override_order() {
    let root = unique_temp_dir();
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("failed to create config directory");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"feature-a\": {},\n    \"feature-b\": {}\n  },\n  \"overrideFeatureInstallOrder\": [\"feature-b\", \"feature-a\"]\n}\n",
    )
    .expect("failed to write config");

    let payload = build_features_resolve_dependencies_payload(&[
        "--workspace-folder".to_string(),
        root.display().to_string(),
    ])
    .expect("payload");

    let features = payload["resolvedFeatures"]
        .as_array()
        .expect("resolved features");
    assert_eq!(features[0], "feature-b");
    assert_eq!(features[1], "feature-a");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_info_reads_manifest_metadata() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create feature root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"demo-feature\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("failed to write feature manifest");

    let payload = build_feature_info_payload("manifest", root.to_string_lossy().as_ref())
        .expect("feature info");

    assert_eq!(payload["id"], "demo-feature");
    assert_eq!(payload["name"], "Demo Feature");
    assert_eq!(payload["version"], "1.0.0");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_info_rejects_unsupported_modes() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create feature root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"demo-feature\"\n}\n",
    )
    .expect("failed to write feature manifest");

    let result = build_feature_info_payload("tags", root.to_string_lossy().as_ref());

    assert_eq!(
        result.expect_err("expected unsupported mode"),
        "Unsupported features info mode: tags"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_info_reads_published_catalog_metadata() {
    let payload =
        build_feature_info_payload("manifest", "ghcr.io/devcontainers/features/azure-cli:1")
            .expect("feature info");

    assert_eq!(payload["id"], "azure-cli");
    assert_eq!(payload["name"], "Azure CLI");
}

#[test]
fn feature_info_supports_generic_published_features() {
    let payload = build_feature_info_payload("manifest", "ghcr.io/devcontainers/features/node")
        .expect("feature info");

    assert_eq!(payload["id"], "node");
    assert_eq!(payload["name"], "Node");
    assert_eq!(payload["version"], "latest");
}

#[test]
fn feature_info_supports_digest_pinned_catalog_refs() {
    let payload = build_feature_info_payload(
        "manifest",
        "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c",
    )
    .expect("feature info");

    assert_eq!(payload["id"], "git-lfs");
    assert_eq!(payload["name"], "Git Lfs");
}
