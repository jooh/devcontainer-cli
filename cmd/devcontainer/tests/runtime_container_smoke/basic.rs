use std::fs;

use crate::support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

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
    assert!(exec_log.contains("/bin/sh -lc echo ready"));
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
    assert!(invocations
        .contains("exec --workdir /custom-target fake-container-id /bin/sh -lc echo ready"));
}
