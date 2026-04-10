//! Smoke tests for compose-backed native runtime builds.

use std::fs;

use crate::support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

#[test]
fn build_uses_compose_for_compose_build_services() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "services:\n  app:\n    image: example/native-compose:test\n    build:\n      context: .\n      dockerfile: Dockerfile\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["imageName"], "example/native-compose:test");

    let invocations = harness.read_invocations();
    assert!(invocations.contains("compose --project-name workspace_devcontainer -f "));
    assert!(invocations.contains(" build --pull app"));
}

#[test]
fn build_returns_default_compose_image_name_for_build_only_services() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "services:\n  app:\n    build:\n      context: .\n      dockerfile: Dockerfile\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["imageName"], "workspace_devcontainer-app");
}

#[test]
fn build_passes_no_cache_to_compose_builds() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "services:\n  app:\n    image: example/native-compose:test\n    build:\n      context: .\n      dockerfile: Dockerfile\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--build-no-cache",
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = harness.read_invocations();
    assert!(invocations.contains(" build --pull --no-cache app"));
}

#[test]
fn compose_build_layers_features_on_top_of_service_images() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    let feature_dir = workspace.join(".devcontainer").join("local-feature");
    fs::create_dir_all(&feature_dir).expect("feature dir");
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        "{\n  \"id\": \"local-feature\",\n  \"name\": \"Local Feature\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .expect("feature manifest");
    fs::write(feature_dir.join("install.sh"), "#!/bin/sh\nset -eu\n").expect("install script");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "services:\n  app:\n    image: example/native-compose:featured\n    build:\n      context: .\n      dockerfile: Dockerfile\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\",\n  \"features\": {\n    \"./local-feature\": {}\n  }\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("compose --project-name workspace_devcontainer -f "));
    assert!(invocations.contains(" build --pull app"));
    assert!(invocations.contains("build --tag example/native-compose:featured"));
}

#[test]
fn build_rejects_cache_to_for_compose_builds() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "services:\n  app:\n    image: example/native-compose:test\n    build:\n      context: .\n      dockerfile: Dockerfile\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--cache-to",
            "type=local,dest=/tmp/compose-cache",
        ],
        &[],
    );

    assert!(!output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stderr)
            .expect("utf8 stderr")
            .trim(),
        "--cache-to not supported for compose builds."
    );
}
