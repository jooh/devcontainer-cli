mod support;

use std::fs;

use support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

#[test]
fn up_starts_a_container_and_exec_runs_inside_it() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"workspaceFolder\": \"/workspace\",\n  \"postCreateCommand\": \"echo ready\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let up_output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--include-configuration",
        ],
        &[("FAKE_PODMAN_PS_DISABLE_DEFAULT", "1")],
    );

    assert!(up_output.status.success(), "{up_output:?}");
    let up_payload = harness.parse_stdout_json(&up_output);
    assert_eq!(up_payload["containerId"], "fake-container-id");
    assert_eq!(up_payload["remoteWorkspaceFolder"], "/workspace");
    assert_eq!(up_payload["configuration"]["image"], "alpine:3.20");

    let exec_output = harness.run(
        &[
            "exec",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "/bin/echo",
            "hello-from-container",
        ],
        &[],
    );

    assert!(exec_output.status.success(), "{exec_output:?}");
    assert_eq!(
        String::from_utf8(exec_output.stdout).expect("utf8 stdout"),
        "hello-from-container\n"
    );

    let invocations = harness.read_invocations();
    assert!(invocations.contains("run "));
    assert!(invocations
        .contains("exec --workdir /workspace fake-container-id /bin/echo hello-from-container"));

    let exec_log = harness.read_exec_log();
    assert!(exec_log.contains("sh -lc echo ready"));
}

#[test]
fn up_uses_workspace_mount_target_for_remote_workdir_when_workspace_folder_is_omitted() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"workspaceMount\": \"type=bind,source=/host/project,target=/custom-target\",\n  \"postCreateCommand\": \"echo ready\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[("FAKE_PODMAN_PS_DISABLE_DEFAULT", "1")],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["remoteWorkspaceFolder"], "/custom-target");
    let invocations = harness.read_invocations();
    assert!(
        invocations.contains("exec --workdir /custom-target fake-container-id sh -lc echo ready")
    );
}

#[test]
fn up_preserves_custom_id_labels_for_followup_exec() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(&workspace, "{\n  \"image\": \"alpine:3.20\"\n}\n");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let up_output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--id-label",
            "example.label=from-user",
        ],
        &[("FAKE_PODMAN_PS_DISABLE_DEFAULT", "1")],
    );

    assert!(up_output.status.success(), "{up_output:?}");

    let exec_output = harness.run(
        &[
            "exec",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--id-label",
            "example.label=from-user",
            "/bin/echo",
            "hello-via-label",
        ],
        &[("FAKE_PODMAN_PS_REQUIRE_LABEL", "example.label=from-user")],
    );

    assert!(exec_output.status.success(), "{exec_output:?}");
    assert_eq!(
        String::from_utf8(exec_output.stdout).expect("utf8 stdout"),
        "hello-via-label\n"
    );

    let invocations = harness.read_invocations();
    assert!(invocations.contains("--label example.label=from-user"));
    assert!(invocations.contains("ps -q --filter label=example.label=from-user"));
}

#[test]
fn up_reuses_existing_container_when_labels_match() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(&workspace, "{\n  \"image\": \"alpine:3.20\"\n}\n");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[("FAKE_PODMAN_PS_OUTPUT", "existing-container-id")],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["containerId"], "existing-container-id");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("ps -q "));
    assert!(!invocations.contains("run "));
}

#[test]
fn up_reusing_running_container_skips_create_only_lifecycle_hooks() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"onCreateCommand\": \"echo on-create\",\n  \"updateContentCommand\": \"echo update-content\",\n  \"postCreateCommand\": \"echo post-create\",\n  \"postStartCommand\": \"echo post-start\",\n  \"postAttachCommand\": \"echo post-attach\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[("FAKE_PODMAN_PS_OUTPUT", "existing-container-id")],
    );

    assert!(output.status.success(), "{output:?}");
    let exec_log = harness.read_exec_log();
    assert!(!exec_log.contains("sh -lc echo on-create"));
    assert!(!exec_log.contains("sh -lc echo update-content"));
    assert!(!exec_log.contains("sh -lc echo post-create"));
    assert!(!exec_log.contains("sh -lc echo post-start"));
    assert!(exec_log.contains("sh -lc echo post-attach"));
}

#[test]
fn up_resumes_stopped_containers_instead_of_creating_new_ones() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"onCreateCommand\": \"echo on-create\",\n  \"updateContentCommand\": \"echo update-content\",\n  \"postCreateCommand\": \"echo post-create\",\n  \"postStartCommand\": \"echo post-start\",\n  \"postAttachCommand\": \"echo post-attach\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[
            ("FAKE_PODMAN_PS_OUTPUT", "stopped-container-id"),
            ("FAKE_PODMAN_PS_REQUIRE_ALL", "1"),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["containerId"], "stopped-container-id");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("ps -q -a "));
    assert!(invocations.contains("start stopped-container-id"));
    assert!(!invocations.contains("run "));
    let exec_log = harness.read_exec_log();
    assert!(!exec_log.contains("sh -lc echo on-create"));
    assert!(!exec_log.contains("sh -lc echo update-content"));
    assert!(!exec_log.contains("sh -lc echo post-create"));
    assert!(exec_log.contains("sh -lc echo post-start"));
    assert!(exec_log.contains("sh -lc echo post-attach"));
}

#[test]
fn up_remove_existing_container_recreates_the_container() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(&workspace, "{\n  \"image\": \"alpine:3.20\"\n}\n");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--remove-existing-container",
        ],
        &[("FAKE_PODMAN_PS_OUTPUT", "existing-container-id")],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["containerId"], "fake-container-id");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("rm -f existing-container-id"));
    assert!(invocations.contains("run "));
}

#[test]
fn up_expect_existing_container_fails_when_missing() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(&workspace, "{\n  \"image\": \"alpine:3.20\"\n}\n");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--expect-existing-container",
        ],
        &[("FAKE_PODMAN_PS_DISABLE_DEFAULT", "1")],
    );

    assert!(!output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stderr)
            .expect("utf8 stderr")
            .trim(),
        "Dev container not found."
    );
}
