use std::fs;

use serde_json::Value;

use crate::support::runtime_harness::RuntimeHarness;
use crate::support::test_support::{devcontainer_command, unique_temp_dir};

const DEFAULT_PUBLISHED_TEMPLATE_BASE_IMAGE: &str = "docker.io/library/debian:bookworm-slim";

#[test]
fn features_test_emits_a_local_report() {
    let harness = RuntimeHarness::new();
    let workspace = harness.root.join("feature-project");
    let src = workspace.join("src").join("demo");
    let test = workspace.join("test").join("demo");
    fs::create_dir_all(&src).expect("feature src");
    fs::create_dir_all(&test).expect("feature test");
    fs::write(
        src.join("devcontainer-feature.json"),
        "{\n  \"id\": \"demo\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("manifest");
    fs::write(test.join("test.sh"), "#!/bin/sh\nexit 0\n").expect("test script");
    fs::write(test.join("custom.sh"), "#!/bin/sh\nexit 0\n").expect("scenario script");
    fs::write(
        test.join("scenarios.json"),
        "{\n  \"custom\": {\n    \"image\": \"ubuntu:latest\"\n  }\n}\n",
    )
    .expect("scenarios");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "features",
            "test",
            "--docker-path",
            fake_podman.as_str(),
            "--project-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("TEST REPORT"));
    assert!(stdout.contains("'custom'"));
    assert!(stdout.contains("'demo'"));
    assert!(stdout.contains("Cleaning up 2 test containers"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn features_test_fails_when_a_test_script_fails() {
    let harness = RuntimeHarness::new();
    let workspace = harness.root.join("feature-project");
    let src = workspace.join("src").join("demo");
    let test = workspace.join("test").join("demo");
    fs::create_dir_all(&src).expect("feature src");
    fs::create_dir_all(&test).expect("feature test");
    fs::write(
        src.join("devcontainer-feature.json"),
        "{\n  \"id\": \"demo\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("manifest");
    fs::write(test.join("test.sh"), "#!/bin/sh\nexit 1\n").expect("test script");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "features",
            "test",
            "--docker-path",
            fake_podman.as_str(),
            "--project-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[("FAKE_PODMAN_EXEC_EXIT_CODE", "1")],
    );

    assert!(!output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("TEST REPORT"));
    assert!(stdout.contains("❌ Failed:      'demo'"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn features_test_quiet_suppresses_local_report_output() {
    let harness = RuntimeHarness::new();
    let workspace = harness.root.join("feature-project");
    let src = workspace.join("src").join("demo");
    let test = workspace.join("test").join("demo");
    fs::create_dir_all(&src).expect("feature src");
    fs::create_dir_all(&test).expect("feature test");
    fs::write(
        src.join("devcontainer-feature.json"),
        "{\n  \"id\": \"demo\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("manifest");
    fs::write(test.join("test.sh"), "#!/bin/sh\nexit 0\n").expect("test script");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "features",
            "test",
            "--docker-path",
            fake_podman.as_str(),
            "--project-folder",
            workspace.to_string_lossy().as_ref(),
            "--quiet",
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("TEST REPORT"));
    assert!(!stdout.contains("Cleaning up"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn templates_apply_supports_published_template_ids() {
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    fs::create_dir_all(&workspace).expect("workspace");

    let output = devcontainer_command(None)
        .args([
            "templates",
            "apply",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--template-id",
            "ghcr.io/devcontainers/templates/docker-from-docker:latest",
            "--template-args",
            "{ \"installZsh\": \"false\", \"upgradePackages\": \"true\", \"dockerVersion\": \"20.10\", \"moby\": \"true\", \"enableNonRootDocker\": \"true\" }",
            "--features",
            "[{ \"id\": \"ghcr.io/devcontainers/features/azure-cli:1\", \"options\": { \"version\": \"1\" } }]",
        ])
        .output()
        .expect("templates apply should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("apply payload");
    assert_eq!(payload["files"][0], "./.devcontainer/devcontainer.json");
    let file = fs::read_to_string(workspace.join(".devcontainer").join("devcontainer.json"))
        .expect("devcontainer file");
    assert!(file.contains("\"name\": \"Docker from Docker\""));
    assert!(file.contains("\"installZsh\": \"false\""));
    assert!(file.contains("\"upgradePackages\": \"true\""));
    assert!(file.contains("\"version\": \"20.10\""));
    assert!(file.contains("\"moby\": \"true\""));
    assert!(file.contains("\"enableNonRootDocker\": \"true\""));
    assert!(file.contains(&format!(
        "\"image\": \"{DEFAULT_PUBLISHED_TEMPLATE_BASE_IMAGE}\""
    )));
    assert!(file.contains("ghcr.io/devcontainers/features/common-utils:1"));
    assert!(file.contains("ghcr.io/devcontainers/features/docker-from-docker:1"));
    assert!(file.contains("ghcr.io/devcontainers/features/azure-cli:1"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn templates_apply_uses_upstream_defaults_for_published_template_ids() {
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    fs::create_dir_all(&workspace).expect("workspace");

    let output = devcontainer_command(None)
        .args([
            "templates",
            "apply",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--template-id",
            "ghcr.io/devcontainers/templates/docker-from-docker:latest",
        ])
        .output()
        .expect("templates apply should run");

    assert!(output.status.success(), "{output:?}");
    let file = fs::read_to_string(workspace.join(".devcontainer").join("devcontainer.json"))
        .expect("devcontainer file");
    assert!(file.contains("\"installZsh\": \"true\""));
    assert!(file.contains("\"upgradePackages\": \"false\""));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn templates_metadata_supports_published_template_ids() {
    let output = devcontainer_command(None)
        .args([
            "templates",
            "metadata",
            "ghcr.io/devcontainers/templates/docker-from-docker:latest",
        ])
        .output()
        .expect("templates metadata should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("metadata payload");
    assert_eq!(payload["id"], "docker-from-docker");
    assert_eq!(payload["name"], "Docker from Docker");
}

#[test]
fn features_info_supports_additional_published_feature_ids() {
    let output = devcontainer_command(None)
        .args([
            "features",
            "info",
            "manifest",
            "ghcr.io/devcontainers/features/node",
        ])
        .output()
        .expect("features info should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("feature info payload");
    assert_eq!(payload["id"], "node");
    assert_eq!(payload["name"], "Node");
}

#[test]
fn features_info_supports_text_output_for_verbose_mode() {
    let output = devcontainer_command(None)
        .args([
            "features",
            "info",
            "verbose",
            "ghcr.io/devcontainers/features/git:1",
            "--output-format",
            "text",
        ])
        .output()
        .expect("features info should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("\"feature\": \"ghcr.io/devcontainers/features/git\""));
    assert!(stdout.contains("\"tags\""));
}

#[test]
fn templates_metadata_supports_additional_published_template_ids() {
    let output = devcontainer_command(None)
        .args([
            "templates",
            "metadata",
            "ghcr.io/devcontainers/templates/anaconda-postgres:latest",
        ])
        .output()
        .expect("templates metadata should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("metadata payload");
    assert_eq!(payload["id"], "anaconda-postgres");
}

#[test]
fn templates_apply_supports_additional_published_template_ids() {
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    fs::create_dir_all(&workspace).expect("workspace");

    let output = devcontainer_command(None)
        .args([
            "templates",
            "apply",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--template-id",
            "ghcr.io/devcontainers/templates/anaconda-postgres:latest",
            "--template-args",
            "{ \"nodeVersion\": \"lts/*\" }",
            "--features",
            "[{ \"id\": \"ghcr.io/devcontainers/features/azure-cli:1\", \"options\": {} }, { \"id\": \"ghcr.io/devcontainers/features/git:1\", \"options\": { \"version\": \"latest\", \"ppa\": true } }]",
        ])
        .output()
        .expect("templates apply should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("apply payload");
    assert_eq!(payload["files"][0], "./.devcontainer/devcontainer.json");
    let file = fs::read_to_string(workspace.join(".devcontainer").join("devcontainer.json"))
        .expect("devcontainer file");
    assert!(file.contains("\"name\": \"Anaconda Postgres\""));
    assert!(file.contains(&format!(
        "\"image\": \"{DEFAULT_PUBLISHED_TEMPLATE_BASE_IMAGE}\""
    )));
    assert!(file.contains("ghcr.io/devcontainers/features/azure-cli:1"));
    assert!(file.contains("ghcr.io/devcontainers/features/git:1"));

    let _ = fs::remove_dir_all(workspace);
}
