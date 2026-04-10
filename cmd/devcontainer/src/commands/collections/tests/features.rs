//! Unit tests for feature collection commands.

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
fn feature_dependency_resolution_rejects_disallowed_features() {
    let root = unique_temp_dir();
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("failed to create config directory");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/problematic-feature:1\": {}\n  }\n}\n",
    )
    .expect("failed to write config");

    let error = build_features_resolve_dependencies_payload(&[
        "--workspace-folder".to_string(),
        root.display().to_string(),
    ])
    .expect_err("disallowed feature should fail");

    assert!(error.contains("problematic-feature:1"), "{error}");
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
fn feature_info_reads_published_catalog_oci_manifest() {
    let payload =
        build_feature_info_payload("manifest", "ghcr.io/devcontainers/features/azure-cli:1")
            .expect("feature info");

    assert_eq!(payload["schemaVersion"], 2);
    assert_eq!(
        payload["mediaType"],
        "application/vnd.oci.image.manifest.v1+json"
    );
    assert_eq!(
        payload["layers"][0]["mediaType"],
        "application/vnd.devcontainers.layer.v1+tar"
    );
    let metadata = payload["annotations"]["dev.containers.metadata"]
        .as_str()
        .expect("metadata string");
    assert!(metadata.contains("\"id\":\"azure-cli\""), "{metadata}");
    assert!(metadata.contains("\"name\":\"Azure CLI\""), "{metadata}");
}

#[test]
fn feature_info_supports_generic_published_features() {
    let payload = build_feature_info_payload("manifest", "ghcr.io/devcontainers/features/node")
        .expect("feature info");

    assert_eq!(
        payload["layers"][0]["annotations"]["org.opencontainers.image.title"],
        "devcontainer-feature-node.tgz"
    );
    let metadata = payload["annotations"]["dev.containers.metadata"]
        .as_str()
        .expect("metadata string");
    assert!(metadata.contains("\"id\":\"node\""), "{metadata}");
    assert!(metadata.contains("\"version\":\"latest\""), "{metadata}");
}

#[test]
fn feature_info_supports_digest_pinned_catalog_refs() {
    let payload = build_feature_info_payload(
        "manifest",
        "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c",
    )
    .expect("feature info");

    assert_eq!(
        payload["layers"][0]["annotations"]["org.opencontainers.image.title"],
        "devcontainer-feature-git-lfs.tgz"
    );
    let metadata = payload["annotations"]["dev.containers.metadata"]
        .as_str()
        .expect("metadata string");
    assert!(metadata.contains("\"id\":\"git-lfs\""), "{metadata}");
    assert!(metadata.contains("\"name\":\"Git Lfs\""), "{metadata}");
}

#[test]
fn feature_info_reports_tags_dependencies_and_verbose_payloads() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create feature root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"demo-feature\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\",\n  \"dependsOn\": {\n    \"ghcr.io/devcontainers/features/common-utils:2\": {}\n  }\n}\n",
    )
    .expect("failed to write feature manifest");

    let tags =
        build_feature_info_payload("tags", root.to_string_lossy().as_ref()).expect("tags payload");
    let dependencies = build_feature_info_payload("dependencies", root.to_string_lossy().as_ref())
        .expect("dependencies payload");
    let verbose = build_feature_info_payload("verbose", root.to_string_lossy().as_ref())
        .expect("verbose payload");

    assert_eq!(tags["tags"][0], "1.0.0");
    assert!(dependencies["dependsOn"]
        .as_object()
        .expect("dependsOn object")
        .contains_key("ghcr.io/devcontainers/features/common-utils:2"));
    assert_eq!(verbose["manifest"]["id"], "demo-feature");
    assert_eq!(verbose["tags"][0], "1.0.0");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_info_reads_catalog_tags_for_published_features() {
    let payload = build_feature_info_payload("tags", "ghcr.io/devcontainers/features/git:1")
        .expect("tags payload");

    let tags = payload["tags"].as_array().expect("tags array");
    assert_eq!(tags[0], "1.2.0");
    assert_eq!(tags[1], "1.1.5");
}
