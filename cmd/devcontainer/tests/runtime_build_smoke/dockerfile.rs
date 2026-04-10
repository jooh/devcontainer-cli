//! Smoke tests for Dockerfile-based native runtime builds.

use std::fs;

use crate::support::runtime_harness::{write_devcontainer_config, RuntimeHarness};

#[test]
fn build_invokes_podman_for_dockerfile_configs() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    write_devcontainer_config(
        &workspace,
        "{\n  \"build\": {\n    \"dockerfile\": \"Dockerfile\",\n    \"context\": \".devcontainer\"\n  }\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--image-name",
            "example/native-build:test",
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let payload = harness.parse_stdout_json(&output);
    assert_eq!(payload["outcome"], "success");
    assert_eq!(payload["imageName"], "example/native-build:test");

    let invocations = harness.read_invocations();
    assert!(invocations.contains("build "));
    assert!(invocations.contains("--tag example/native-build:test"));
    assert!(invocations.contains("--file"));
}

#[test]
fn build_passes_configured_build_args_to_the_engine() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\nARG VARIANT\nARG TOOLCHAIN\n",
    )
    .expect("dockerfile");
    write_devcontainer_config(
        &workspace,
        "{\n  \"build\": {\n    \"dockerfile\": \"Dockerfile\",\n    \"context\": \".devcontainer\",\n    \"args\": {\n      \"VARIANT\": \"bookworm\",\n      \"TOOLCHAIN\": \"stable\"\n    }\n  }\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--image-name",
            "example/native-build:args",
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("--build-arg VARIANT=bookworm"));
    assert!(invocations.contains("--build-arg TOOLCHAIN=stable"));
}

#[test]
fn build_never_buildkit_sets_engine_env() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    write_devcontainer_config(
        &workspace,
        "{\n  \"build\": {\n    \"dockerfile\": \"Dockerfile\",\n    \"context\": \".devcontainer\"\n  }\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "build",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--buildkit",
            "never",
        ],
        &[],
    );

    assert!(output.status.success(), "{output:?}");
    let build_env =
        fs::read_to_string(harness.log_dir.join("build-env.log")).expect("build env log");
    assert!(build_env.contains("DOCKER_BUILDKIT=0"));
}

#[test]
fn up_honors_build_no_cache() {
    let harness = RuntimeHarness::new();
    let workspace = harness.workspace();
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::write(
        workspace.join(".devcontainer").join("Dockerfile"),
        "FROM scratch\n",
    )
    .expect("dockerfile");
    write_devcontainer_config(
        &workspace,
        "{\n  \"build\": {\n    \"dockerfile\": \"Dockerfile\",\n    \"context\": \".devcontainer\"\n  }\n}\n",
    );

    let fake_podman = harness.fake_podman.to_string_lossy().to_string();
    let output = harness.run(
        &[
            "up",
            "--docker-path",
            fake_podman.as_str(),
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--build-no-cache",
        ],
        &[("FAKE_PODMAN_PS_DISABLE_DEFAULT", "1")],
    );

    assert!(output.status.success(), "{output:?}");
    let invocations = harness.read_invocations();
    assert!(invocations.contains("build "));
    assert!(invocations.contains("--no-cache"));
    if cfg!(target_os = "linux") {
        assert!(invocations.contains("image inspect --format"));
    }
}
