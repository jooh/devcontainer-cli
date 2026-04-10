//! Runtime container smoke tests for compose project-name behavior.

use std::fs;

use crate::support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

#[test]
fn up_uses_custom_compose_project_name_from_compose_file() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "name: Custom-Project-Name\nservices:\n  app:\n    image: alpine:3.20\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
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
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["composeProjectName"], "custom-project-name");

    let invocations = harness.read_invocations();
    assert!(invocations.contains("compose --project-name custom-project-name -f "));
}

#[test]
fn up_reads_compose_project_name_from_compose_directory_dotenv() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    let outside = harness.root.join("outside");
    fs::create_dir_all(&outside).expect("outside dir");
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join(".env"),
        "COMPOSE_PROJECT_NAME=dotenv-project\n",
    )
    .expect("dotenv");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.20\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run_in_dir(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ],
        &[],
        Some(&outside),
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["composeProjectName"], "dotenv-project");

    let invocations = harness.read_invocations();
    assert!(invocations.contains("compose --project-name dotenv-project -f "));
}

#[test]
fn up_expands_plain_dollar_compose_project_names() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "name: $CUSTOM_PROJECT\nservices:\n  app:\n    image: alpine:3.20\n",
    )
    .expect("compose");
    write_devcontainer_config(
        &workspace,
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
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
        &[("CUSTOM_PROJECT", "FromEnv_Project")],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["composeProjectName"], "fromenv_project");

    let invocations = harness.read_invocations();
    assert!(invocations.contains("compose --project-name fromenv_project -f "));
}
