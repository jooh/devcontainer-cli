//! Smoke tests for lifecycle selection, wait-for, and skip behavior.

use std::fs;

use crate::support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

#[test]
fn up_runs_initialize_command_before_starting_container() {
    let harness = RuntimeHarness::new();
    let initialize_marker = harness.root.join("initialize.marker");
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        &format!(
            "{{\n  \"image\": \"alpine:3.20\",\n  \"initializeCommand\": \"printf initialized > {}\"\n}}\n",
            initialize_marker.display()
        ),
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
            (
                "FAKE_PODMAN_REQUIRE_FILE_BEFORE_RUN",
                initialize_marker.to_string_lossy().as_ref(),
            ),
            ("FAKE_PODMAN_PS_DISABLE_DEFAULT", "1"),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(&initialize_marker).expect("initialize marker"),
        "initialized"
    );
}

#[test]
fn skip_post_create_skips_post_start_and_post_attach() {
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
            "--skip-post-create",
        ],
        &[("FAKE_PODMAN_PS_DISABLE_DEFAULT", "1")],
    );

    assert!(output.status.success(), "{output:?}");
    assert!(!harness.log_dir.join("exec.log").exists());
}

#[test]
fn skip_non_blocking_stops_after_default_wait_for() {
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
            "run-user-commands",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--skip-non-blocking-commands",
        ],
        &[("FAKE_PODMAN_PS_OUTPUT", "fake-container-id")],
    );

    assert!(output.status.success(), "{output:?}");
    let exec_log = harness.read_exec_log();
    assert!(exec_log.contains("/bin/sh -lc echo on-create"));
    assert!(exec_log.contains("/bin/sh -lc echo update-content"));
    assert!(!exec_log.contains("/bin/sh -lc echo post-create"));
    assert!(!exec_log.contains("/bin/sh -lc echo post-start"));
    assert!(!exec_log.contains("/bin/sh -lc echo post-attach"));
}

#[test]
fn skip_non_blocking_respects_wait_for_post_start() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"waitFor\": \"postStartCommand\",\n  \"onCreateCommand\": \"echo on-create\",\n  \"updateContentCommand\": \"echo update-content\",\n  \"postCreateCommand\": \"echo post-create\",\n  \"postStartCommand\": \"echo post-start\",\n  \"postAttachCommand\": \"echo post-attach\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "run-user-commands",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--skip-non-blocking-commands",
        ],
        &[("FAKE_PODMAN_PS_OUTPUT", "fake-container-id")],
    );

    assert!(output.status.success(), "{output:?}");
    let exec_log = harness.read_exec_log();
    assert!(exec_log.contains("/bin/sh -lc echo on-create"));
    assert!(exec_log.contains("/bin/sh -lc echo update-content"));
    assert!(exec_log.contains("/bin/sh -lc echo post-create"));
    assert!(exec_log.contains("/bin/sh -lc echo post-start"));
    assert!(!exec_log.contains("/bin/sh -lc echo post-attach"));
}
