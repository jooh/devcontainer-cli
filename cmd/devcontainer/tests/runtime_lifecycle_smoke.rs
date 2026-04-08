mod support;

use serde_json::json;
use std::fs;

use support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

#[test]
fn run_user_commands_resolves_container_ids_from_headered_ps_output() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"postCreateCommand\": \"echo post-create\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "run-user-commands",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[
            ("FAKE_PODMAN_PS_OUTPUT", "fake-container-id"),
            ("FAKE_PODMAN_PS_WITH_HEADER", "1"),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("ps -q "));
    let exec_log = harness.read_exec_log();
    assert!(exec_log.contains("/bin/sh -lc echo post-create"));
}

#[test]
fn run_user_commands_with_container_id_loads_metadata_lifecycle_hooks() {
    let harness = RuntimeHarness::new();
    let inspect_path = harness.root.join("inspect.json");
    let metadata = serde_json::to_string(&json!({
        "postCreateCommand": "echo post-create-from-metadata",
        "postAttachCommand": "echo post-attach-from-metadata",
        "workspaceFolder": "/metadata-workspace"
    }))
    .expect("metadata");
    fs::write(
        &inspect_path,
        json!([{
            "Config": {
                "Labels": {
                    "devcontainer.metadata": metadata
                }
            },
            "Mounts": []
        }])
        .to_string(),
    )
    .expect("inspect json");

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "run-user-commands",
            "--docker-path",
            fake_podman.as_str(),
            "--container-id",
            "fake-container-id",
        ],
        &[(
            "FAKE_PODMAN_INSPECT_FILE",
            inspect_path.to_string_lossy().as_ref(),
        )],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["remoteWorkspaceFolder"], "/metadata-workspace");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("inspect fake-container-id"));
    let exec_log = harness.read_exec_log();
    assert!(exec_log.contains("/bin/sh -lc echo post-create-from-metadata"));
    assert!(exec_log.contains("/bin/sh -lc echo post-attach-from-metadata"));
}

#[test]
fn lifecycle_commands_run_as_the_configured_remote_user() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"remoteUser\": \"vscode\",\n  \"postCreateCommand\": \"echo ready\"\n}\n",
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
    let invocations = harness.read_invocations();
    assert!(invocations.contains("exec --workdir /workspaces/workspace --user vscode"));
}

#[test]
fn set_up_and_run_user_commands_target_existing_containers() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    let config_path = write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"postCreateCommand\": \"echo post-create\",\n  \"postAttachCommand\": \"echo post-attach\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let set_up_output = harness.run(
        &[
            "set-up",
            "--docker-path",
            fake_podman.as_str(),
            "--container-id",
            "fake-container-id",
            "--config",
            config_path.to_string_lossy().as_ref(),
            "--include-configuration",
        ],
        &[],
    );

    assert!(set_up_output.status.success(), "{set_up_output:?}");
    let payload = harness.parse_stdout_json(&set_up_output);
    assert_eq!(payload["containerId"], "fake-container-id");
    assert_eq!(payload["configuration"]["image"], "alpine:3.20");

    let run_user_commands_output = harness.run(
        &[
            "run-user-commands",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[("FAKE_PODMAN_PS_OUTPUT", "fake-container-id")],
    );

    assert!(
        run_user_commands_output.status.success(),
        "{run_user_commands_output:?}"
    );
    let exec_log = harness.read_exec_log();
    assert!(exec_log.contains("/bin/sh -lc echo post-create"));
    assert!(exec_log.contains("/bin/sh -lc echo post-attach"));
}

#[test]
fn compose_lifecycle_commands_honor_explicit_container_id() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.20\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\",\n  \"postCreateCommand\": \"echo post-create\",\n  \"postAttachCommand\": \"echo post-attach\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let set_up_output = harness.run(
        &[
            "set-up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--container-id",
            "fake-compose-container-id",
            "--include-configuration",
        ],
        &[("FAKE_PODMAN_COMPOSE_PS_OUTPUT", "")],
    );

    assert!(set_up_output.status.success(), "{set_up_output:?}");
    let payload = harness.parse_stdout_json(&set_up_output);
    assert_eq!(payload["containerId"], "fake-compose-container-id");
    assert_eq!(payload["configuration"]["service"], "app");

    let run_user_commands_output = harness.run(
        &[
            "run-user-commands",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--container-id",
            "fake-compose-container-id",
        ],
        &[("FAKE_PODMAN_COMPOSE_PS_OUTPUT", "")],
    );

    assert!(
        run_user_commands_output.status.success(),
        "{run_user_commands_output:?}"
    );
    let invocations = harness.read_invocations();
    assert!(!invocations.contains("compose --project-name"));
    assert!(invocations.contains(
        "exec --workdir /workspace fake-compose-container-id /bin/sh -lc echo post-create"
    ));
    assert!(invocations.contains(
        "exec --workdir /workspace fake-compose-container-id /bin/sh -lc echo post-attach"
    ));
    let exec_log = harness.read_exec_log();
    assert!(exec_log.contains("/bin/sh -lc echo post-create"));
    assert!(exec_log.contains("/bin/sh -lc echo post-attach"));
}

#[test]
fn lifecycle_array_commands_preserve_argument_boundaries() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"postCreateCommand\": [\"printf\", \"%s\", \"foo='bar baz'\"]\n}\n",
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
    let exec_argv = harness.read_exec_argv_log();
    assert!(exec_argv.contains("[printf]\n[%s]\n[foo='bar baz']"));
    assert!(!exec_argv.contains("[sh]\n[-lc]\n[printf %s foo='bar baz']"));
}

#[test]
fn object_lifecycle_commands_are_executed() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(&workspace).expect("workspace dir");
    let config_path = write_devcontainer_config(
        &workspace,
        "{\n  \"image\": \"alpine:3.20\",\n  \"postCreateCommand\": {\n    \"alpha\": \"echo first\",\n    \"beta\": [\"printf\", \"%s\", \"second value\"]\n  }\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "set-up",
            "--docker-path",
            fake_podman.as_str(),
            "--container-id",
            "fake-container-id",
            "--config",
            config_path.to_string_lossy().as_ref(),
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = harness.read_invocations();
    assert!(invocations
        .contains("exec --workdir /workspaces/workspace fake-container-id /bin/sh -lc echo first"));
    assert!(invocations
        .contains("exec --workdir /workspaces/workspace fake-container-id printf %s second value"));
}

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
