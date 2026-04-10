//! Unit tests for feature publishing helpers.

use std::fs;

use super::support::unique_temp_dir;
use crate::commands::collections::publish::publish_collection_target_to_oci;
use crate::commands::common::{
    generate_manifest_docs, package_collection_target, ManifestDocOptions,
};

#[test]
fn packaging_a_collection_target_creates_an_archive() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create package root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"packaged-feature\",\n  \"name\": \"Packaged Feature\"\n}\n",
    )
    .expect("failed to write feature manifest");

    let archive =
        package_collection_target(&root, "devcontainer-feature.json", "feature").expect("archive");

    assert!(archive.is_file());
    let _ = fs::remove_file(archive);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn generate_feature_docs_writes_readme() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create docs root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"docs-feature\",\n  \"name\": \"Docs Feature\",\n  \"description\": \"Generated docs\"\n}\n",
    )
    .expect("failed to write feature manifest");

    let readme = generate_manifest_docs(
        &root,
        "devcontainer-feature.json",
        "Feature",
        &ManifestDocOptions::default(),
    )
    .expect("readme");

    assert!(readme.is_file());
    let content = fs::read_to_string(readme).expect("readme content");
    assert!(content.contains("Docs Feature"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn publish_writes_a_local_oci_layout() {
    let root = unique_temp_dir();
    let output_dir = unique_temp_dir();
    fs::create_dir_all(&root).expect("feature root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"published-feature\",\n  \"name\": \"Published Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("manifest");

    let payload = publish_collection_target_to_oci(
        &root,
        "devcontainer-feature.json",
        "feature",
        "features publish",
        &["--output-dir".to_string(), output_dir.display().to_string()],
    )
    .expect("publish payload");

    assert_eq!(payload["published"], true);
    assert_eq!(
        payload["publishedTags"],
        serde_json::json!(["1", "1.0", "1.0.0", "latest"])
    );
    assert!(output_dir.join("oci-layout").is_file());
    assert!(output_dir.join("index.json").is_file());
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(output_dir);
}

#[test]
fn publish_updates_moving_semantic_tags_for_new_patch_versions() {
    let root = unique_temp_dir();
    let output_dir = unique_temp_dir();
    fs::create_dir_all(&root).expect("feature root");
    let manifest_path = root.join("devcontainer-feature.json");
    fs::write(
        &manifest_path,
        "{\n  \"id\": \"published-feature\",\n  \"name\": \"Published Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("manifest");

    publish_collection_target_to_oci(
        &root,
        "devcontainer-feature.json",
        "feature",
        "features publish",
        &["--output-dir".to_string(), output_dir.display().to_string()],
    )
    .expect("first publish payload");

    fs::write(
        &manifest_path,
        "{\n  \"id\": \"published-feature\",\n  \"name\": \"Published Feature\",\n  \"version\": \"1.0.1\"\n}\n",
    )
    .expect("updated manifest");

    let payload = publish_collection_target_to_oci(
        &root,
        "devcontainer-feature.json",
        "feature",
        "features publish",
        &["--output-dir".to_string(), output_dir.display().to_string()],
    )
    .expect("second publish payload");

    assert_eq!(
        payload["publishedTags"],
        serde_json::json!(["1", "1.0", "1.0.1", "latest"])
    );

    let index: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_dir.join("index.json")).expect("index"))
            .expect("index json");
    let tags = index["manifests"]
        .as_array()
        .expect("manifests")
        .iter()
        .filter_map(|entry| entry["annotations"]["org.opencontainers.image.ref.name"].as_str())
        .collect::<Vec<_>>();

    assert!(tags.contains(&"1"));
    assert!(tags.contains(&"1.0"));
    assert!(tags.contains(&"1.0.0"));
    assert!(tags.contains(&"1.0.1"));
    assert!(tags.contains(&"latest"));
    assert_eq!(tags.len(), 5);

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(output_dir);
}

#[test]
fn generate_feature_docs_include_collection_and_repository_metadata() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create docs root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"docs-feature\",\n  \"name\": \"Docs Feature\",\n  \"description\": \"Generated docs\"\n}\n",
    )
    .expect("failed to write feature manifest");

    let readme = generate_manifest_docs(
        &root,
        "devcontainer-feature.json",
        "Feature",
        &ManifestDocOptions {
            registry: Some("ghcr.io".to_string()),
            namespace: Some("devcontainers/features".to_string()),
            github_owner: Some("devcontainers".to_string()),
            github_repo: Some("cli".to_string()),
        },
    )
    .expect("readme");

    let content = fs::read_to_string(readme).expect("readme content");
    assert!(content.contains("`ghcr.io/devcontainers/features/docs-feature`"));
    assert!(content.contains("https://github.com/devcontainers/cli"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn publish_records_registry_namespace_and_resource_metadata() {
    let root = unique_temp_dir();
    let output_dir = unique_temp_dir();
    fs::create_dir_all(&root).expect("feature root");
    fs::write(
        root.join("devcontainer-feature.json"),
        "{\n  \"id\": \"published-feature\",\n  \"name\": \"Published Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("manifest");

    let payload = publish_collection_target_to_oci(
        &root,
        "devcontainer-feature.json",
        "feature",
        "features publish",
        &[
            "--output-dir".to_string(),
            output_dir.display().to_string(),
            "--registry".to_string(),
            "example.registry".to_string(),
            "--namespace".to_string(),
            "acme/features".to_string(),
        ],
    )
    .expect("publish payload");

    assert_eq!(payload["registry"], "example.registry");
    assert_eq!(payload["namespace"], "acme/features");
    assert_eq!(
        payload["resource"],
        "example.registry/acme/features/published-feature"
    );
    let manifest_digest = payload["digest"]
        .as_str()
        .expect("digest")
        .trim_start_matches("sha256:");
    let manifest = fs::read_to_string(
        output_dir
            .join("blobs")
            .join("sha256")
            .join(manifest_digest),
    )
    .expect("manifest blob");
    assert!(manifest.contains("example.registry/acme/features/published-feature"));
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(output_dir);
}
