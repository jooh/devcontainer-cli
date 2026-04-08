use std::fs;

use serde_json::json;

use super::support::unique_temp_dir;
use crate::commands::common::resolve_read_configuration_path;
use crate::commands::configuration::merge::merge_configuration;
use crate::commands::configuration::{
    build_read_configuration_payload, should_use_native_read_configuration,
};

#[test]
fn resolves_modern_config_path_from_workspace_folder() {
    let root = unique_temp_dir();
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("failed to create config directory");
    let config = config_dir.join("devcontainer.json");
    fs::write(&config, "{}").expect("failed to write config");

    let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
    let result = resolve_read_configuration_path(&args).expect("expected config resolution");

    assert_eq!(
        result.1,
        fs::canonicalize(config).expect("failed to canonicalize")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn fails_when_explicit_config_file_is_missing() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create root");
    let missing_config = root.join("missing.json");
    let args = vec![
        "--workspace-folder".to_string(),
        root.display().to_string(),
        "--config".to_string(),
        missing_config.display().to_string(),
    ];

    let result = resolve_read_configuration_path(&args);

    assert!(result.is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn fails_when_workspace_folder_option_is_missing_a_value() {
    let result = resolve_read_configuration_path(&["--workspace-folder".to_string()]);

    assert_eq!(
        result.expect_err("expected missing option value"),
        "Missing value for option: --workspace-folder"
    );
}

#[test]
fn resolves_relative_config_against_workspace_folder() {
    let root = unique_temp_dir();
    let config = root.join("relative.devcontainer.json");
    fs::create_dir_all(&root).expect("failed to create root");
    fs::write(&config, "{}").expect("failed to write config");

    let args = vec![
        "--workspace-folder".to_string(),
        root.display().to_string(),
        "--config".to_string(),
        "relative.devcontainer.json".to_string(),
    ];
    let result = resolve_read_configuration_path(&args).expect("expected config resolution");

    assert_eq!(
        result.1,
        fs::canonicalize(config).expect("failed to canonicalize")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn infers_workspace_root_for_nested_devcontainer_configs() {
    let root = unique_temp_dir();
    let nested_config_dir = root.join(".devcontainer").join("python");
    let config = nested_config_dir.join("devcontainer.json");
    fs::create_dir_all(&root).expect("failed to create root");
    fs::create_dir_all(&nested_config_dir).expect("failed to create nested config directory");
    fs::write(&config, "{}").expect("failed to write config");

    let args = vec!["--config".to_string(), config.display().to_string()];
    let (workspace_folder, config_file) =
        resolve_read_configuration_path(&args).expect("expected config resolution");

    assert_eq!(
        workspace_folder,
        fs::canonicalize(&root).expect("failed to canonicalize workspace")
    );
    assert_eq!(
        config_file,
        fs::canonicalize(config).expect("failed to canonicalize config")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn read_configuration_with_additional_flags_is_supported_natively() {
    assert!(should_use_native_read_configuration(&[
        "--workspace-folder".to_string(),
        "/workspace".to_string(),
        "--include-merged-configuration".to_string(),
    ]));
}

#[test]
fn read_configuration_accepts_docker_compose_path_flag() {
    assert!(should_use_native_read_configuration(&[
        "--workspace-folder".to_string(),
        "/workspace".to_string(),
        "--docker-compose-path".to_string(),
        "trigger-compose-v2".to_string(),
    ]));
}

#[test]
fn read_configuration_accepts_feature_resolution_flags() {
    assert!(should_use_native_read_configuration(&[
        "--workspace-folder".to_string(),
        "/workspace".to_string(),
        "--include-features-configuration".to_string(),
        "--additional-features".to_string(),
        "{\"ghcr.io/devcontainers/features/git:1\":{}}".to_string(),
        "--skip-feature-auto-mapping".to_string(),
    ]));
}

#[test]
fn read_configuration_payload_includes_optional_sections() {
    let root = unique_temp_dir();
    let config_dir = root.join(".devcontainer");
    fs::create_dir_all(&config_dir).expect("failed to create config directory");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\",\n  \"features\": { \"ghcr.io/devcontainers/features/git:1\": {} }\n}\n",
    )
    .expect("failed to write config");

    let args = vec![
        "--workspace-folder".to_string(),
        root.display().to_string(),
        "--include-merged-configuration".to_string(),
        "--include-features-configuration".to_string(),
    ];
    let payload = build_read_configuration_payload(&args).expect("payload");

    assert_eq!(payload["configuration"]["image"], "debian:bookworm");
    assert_eq!(payload["mergedConfiguration"]["image"], "debian:bookworm");
    let feature_sets = payload["featuresConfiguration"]["featureSets"]
        .as_array()
        .expect("feature sets");
    assert_eq!(feature_sets.len(), 1);
    assert_eq!(feature_sets[0]["features"][0]["id"], "git");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn read_configuration_resolves_feature_sets_and_feature_metadata() {
    let root = unique_temp_dir();
    let config_dir = root.join(".devcontainer");
    let local_feature_dir = config_dir.join("local-feature");
    fs::create_dir_all(&local_feature_dir).expect("failed to create local feature directory");
    fs::write(
        local_feature_dir.join("devcontainer-feature.json"),
        "{\n  \"id\": \"local-feature\",\n  \"name\": \"Local Feature\",\n  \"version\": \"1.0.0\",\n  \"options\": {\n    \"favorite\": {\n      \"type\": \"string\",\n      \"default\": \"blue\"\n    }\n  },\n  \"containerEnv\": {\n    \"LOCAL_FEATURE_ENV\": \"enabled\"\n  },\n  \"init\": true,\n  \"customizations\": {\n    \"vscode\": {\n      \"extensions\": [\"ms-vscode.makefile-tools\"]\n    }\n  }\n}\n",
    )
    .expect("failed to write local feature manifest");
    fs::write(
        config_dir.join("devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"./local-feature\": {\n      \"favorite\": \"red\"\n    }\n  },\n  \"containerEnv\": {\n    \"CONFIG_ENV\": \"present\"\n  }\n}\n",
    )
    .expect("failed to write config");

    let args = vec![
        "--workspace-folder".to_string(),
        root.display().to_string(),
        "--include-merged-configuration".to_string(),
        "--include-features-configuration".to_string(),
        "--additional-features".to_string(),
        "{\"ghcr.io/devcontainers/features/git:1\":{\"version\":\"latest\"}}".to_string(),
    ];
    let payload = build_read_configuration_payload(&args).expect("payload");

    let feature_sets = payload["featuresConfiguration"]["featureSets"]
        .as_array()
        .expect("feature sets");
    assert_eq!(feature_sets.len(), 2);
    assert_eq!(feature_sets[0]["features"][0]["id"], "local-feature");
    assert_eq!(feature_sets[0]["features"][0]["options"]["favorite"], "red");
    assert_eq!(feature_sets[0]["sourceInformation"]["type"], "file-path");
    assert_eq!(feature_sets[1]["features"][0]["id"], "git");
    assert_eq!(
        payload["mergedConfiguration"]["containerEnv"]["LOCAL_FEATURE_ENV"],
        "enabled"
    );
    assert_eq!(
        payload["mergedConfiguration"]["containerEnv"]["CONFIG_ENV"],
        "present"
    );
    assert_eq!(payload["mergedConfiguration"]["init"], true);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn merged_configuration_normalizes_forward_ports_before_deduplication() {
    let merged = merge_configuration(
        &json!({ "image": "debian:bookworm" }),
        &[
            json!({ "forwardPorts": [3000] }),
            json!({ "forwardPorts": ["localhost:3000", "0.0.0.0:4000"] }),
        ],
    );

    assert_eq!(merged["forwardPorts"], json!([3000, "0.0.0.0:4000"]));
}

#[test]
fn merged_configuration_merges_host_requirements_field_by_field() {
    let merged = merge_configuration(
        &json!({ "image": "debian:bookworm" }),
        &[
            json!({ "hostRequirements": { "cpus": 2, "gpu": "optional" } }),
            json!({ "hostRequirements": { "memory": "4gb" } }),
            json!({ "hostRequirements": { "gpu": { "cores": 4 } } }),
        ],
    );

    assert_eq!(
        merged["hostRequirements"],
        json!({
            "cpus": 2.0,
            "memory": "4000000000",
            "gpu": {
                "cores": 4
            }
        })
    );
}
