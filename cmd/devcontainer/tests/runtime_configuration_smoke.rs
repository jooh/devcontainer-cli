mod support;

use std::fs;

use support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

#[test]
fn read_configuration_with_container_id_merges_config_and_container_metadata() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    let config_path = write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"postAttachCommand\": \"touch /postAttachCommand.txt\",\n  \"remoteEnv\": {\n    \"TEST_RE\": \"${containerEnv:TEST_CE}\"\n  }\n}\n",
    );

    let inspect_path = harness.root.join("inspect.json");
    fs::write(
        &inspect_path,
        r#"[{
  "Config": {
    "Labels": {
      "devcontainer.metadata": "{ \"postCreateCommand\": \"touch /postCreateCommand.txt\", \"remoteEnv\": { \"FROM_METADATA\": \"yes\" } }"
    },
    "Env": [
      "PATH=/usr/local/bin:/usr/bin",
      "TEST_CE=from-container"
    ]
  },
  "Mounts": [{
    "Source": "/workspace",
    "Destination": "/workspaces/workspace"
  }]
}]"#,
    )
    .expect("inspect file");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "read-configuration",
            "--docker-path",
            fake_podman.as_str(),
            "--container-id",
            "fake-existing-container",
            "--config",
            config_path.to_string_lossy().as_ref(),
            "--include-merged-configuration",
        ],
        &[(
            "FAKE_PODMAN_INSPECT_FILE",
            inspect_path.to_string_lossy().as_ref(),
        )],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(
        payload["configuration"]["remoteEnv"]["TEST_RE"],
        "from-container"
    );
    assert_eq!(
        payload["mergedConfiguration"]["postAttachCommands"]
            .as_array()
            .expect("post attach commands")
            .len(),
        1
    );
    assert_eq!(
        payload["mergedConfiguration"]["postCreateCommands"]
            .as_array()
            .expect("post create commands")
            .len(),
        1
    );
    assert_eq!(
        payload["mergedConfiguration"]["remoteEnv"]["FROM_METADATA"],
        "yes"
    );
}

#[test]
fn read_configuration_with_container_id_uses_container_metadata_without_config() {
    let harness = RuntimeHarness::new();
    let inspect_path = harness.root.join("inspect.json");
    fs::write(
        &inspect_path,
        r#"[{
  "Config": {
    "Labels": {
      "devcontainer.local_folder": "/tmp/workspace",
      "devcontainer.metadata": "{ \"postCreateCommand\": \"touch /postCreateCommand.txt\", \"workspaceFolder\": \"/workspace/from-metadata\" }"
    },
    "Env": [
      "PATH=/usr/local/bin:/usr/bin"
    ]
  },
  "Mounts": [{
    "Source": "/tmp/workspace",
    "Destination": "/workspace/from-metadata"
  }]
}]"#,
    )
    .expect("inspect file");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "read-configuration",
            "--docker-path",
            fake_podman.as_str(),
            "--container-id",
            "fake-existing-container",
            "--include-merged-configuration",
        ],
        &[(
            "FAKE_PODMAN_INSPECT_FILE",
            inspect_path.to_string_lossy().as_ref(),
        )],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["configuration"], serde_json::json!({}));
    assert_eq!(
        payload["mergedConfiguration"]["postCreateCommands"]
            .as_array()
            .expect("post create commands")
            .len(),
        1
    );
    assert!(payload.get("workspace").is_none());
    assert_eq!(
        payload["mergedConfiguration"]["workspaceFolder"],
        "/workspace/from-metadata"
    );
}
