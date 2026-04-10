//! Unit tests for template collection helpers.

use std::fs;

use serde_json::json;

use super::support::unique_temp_dir;
use crate::commands::collections::templates::{
    apply_catalog_template, apply_template_target, build_template_metadata_payload,
    run_template_apply,
};

#[test]
fn published_embedded_templates_copy_upstream_source_files() {
    let workspace = unique_temp_dir();
    fs::create_dir_all(&workspace).expect("workspace");

    let payload = apply_catalog_template(
        "ghcr.io/devcontainers/templates/node-mongo:latest",
        &workspace,
        &[],
    )
    .expect("template apply");

    assert_eq!(payload["id"], "node-mongo");
    assert!(workspace
        .join(".devcontainer")
        .join("docker-compose.yml")
        .is_file());
    assert!(workspace
        .join(".devcontainer")
        .join("devcontainer.json")
        .is_file());
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn published_embedded_templates_apply_template_args_and_extra_features() {
    let workspace = unique_temp_dir();
    fs::create_dir_all(&workspace).expect("workspace");

    apply_catalog_template(
        "ghcr.io/devcontainers/templates/alpine:latest",
        &workspace,
        &[
            "--template-args".to_string(),
            json!({ "imageVariant": "3.14" }).to_string(),
            "--features".to_string(),
            json!([{ "id": "ghcr.io/devcontainers/features/git:1", "options": {} }]).to_string(),
        ],
    )
    .expect("template apply");

    let config = fs::read_to_string(workspace.join(".devcontainer.json")).expect("config");
    assert!(config.contains("0-alpine-3.14"));
    assert!(config.contains("ghcr.io/devcontainers/features/git:1"));
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn template_apply_copies_template_src_into_workspace() {
    let template_root = unique_temp_dir();
    let template_src = template_root.join("src");
    let workspace_root = unique_temp_dir();
    fs::create_dir_all(&template_src).expect("failed to create template src");
    fs::write(
        template_root.join("devcontainer-template.json"),
        "{\n  \"id\": \"demo-template\",\n  \"name\": \"Demo Template\"\n}\n",
    )
    .expect("failed to write template manifest");
    fs::write(template_src.join("README.md"), "# template\n")
        .expect("failed to write template file");

    apply_template_target(&template_root, &workspace_root).expect("apply template");

    assert!(workspace_root.join("README.md").is_file());
    let _ = fs::remove_dir_all(template_root);
    let _ = fs::remove_dir_all(workspace_root);
}

#[test]
fn template_apply_supports_omit_paths_and_tmp_dir() {
    let template_root = unique_temp_dir();
    let template_src = template_root.join("src");
    let workspace_root = unique_temp_dir();
    let tmp_dir = unique_temp_dir();
    fs::create_dir_all(template_src.join(".github")).expect("failed to create template src");
    fs::write(
        template_root.join("devcontainer-template.json"),
        "{\n  \"id\": \"demo-template\",\n  \"name\": \"Demo Template\"\n}\n",
    )
    .expect("failed to write template manifest");
    fs::write(template_src.join("README.md"), "# template\n")
        .expect("failed to write template file");
    fs::write(
        template_src.join(".github").join("workflows.yml"),
        "name: ci\n",
    )
    .expect("failed to write omitted file");

    run_template_apply(&[
        template_root.display().to_string(),
        "--workspace-folder".to_string(),
        workspace_root.display().to_string(),
        "--omit-paths".to_string(),
        "[\".github/*\"]".to_string(),
        "--tmp-dir".to_string(),
        tmp_dir.display().to_string(),
    ])
    .expect("apply template");

    assert!(workspace_root.join("README.md").is_file());
    assert!(!workspace_root.join(".github").exists());
    assert!(tmp_dir.is_dir());
    let _ = fs::remove_dir_all(template_root);
    let _ = fs::remove_dir_all(workspace_root);
    let _ = fs::remove_dir_all(tmp_dir);
}

#[test]
fn template_metadata_reads_manifest_metadata() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create template root");
    fs::write(
        root.join("devcontainer-template.json"),
        "{\n  \"id\": \"demo-template\",\n  \"name\": \"Demo Template\"\n}\n",
    )
    .expect("failed to write template manifest");

    let payload = build_template_metadata_payload(root.to_string_lossy().as_ref())
        .expect("template metadata");

    assert_eq!(payload["id"], "demo-template");
    assert_eq!(payload["name"], "Demo Template");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn template_metadata_reads_published_catalog_metadata() {
    let payload = build_template_metadata_payload(
        "ghcr.io/devcontainers/templates/docker-from-docker:latest",
    )
    .expect("template metadata");

    assert_eq!(payload["id"], "docker-from-docker");
    assert_eq!(payload["name"], "Docker from Docker");
}

#[test]
fn template_metadata_supports_generic_published_templates() {
    let payload =
        build_template_metadata_payload("ghcr.io/devcontainers/templates/anaconda-postgres:latest")
            .expect("template metadata");

    assert_eq!(payload["id"], "anaconda-postgres");
    assert_eq!(payload["name"], "Anaconda Postgres");
}

#[test]
fn template_metadata_supports_digest_pinned_catalog_refs() {
    let payload = build_template_metadata_payload(
        "ghcr.io/devcontainers/templates/docker-from-docker@sha256:0123456789abcdef",
    )
    .expect("template metadata");

    assert_eq!(payload["id"], "docker-from-docker");
    assert_eq!(payload["name"], "Docker from Docker");
}
