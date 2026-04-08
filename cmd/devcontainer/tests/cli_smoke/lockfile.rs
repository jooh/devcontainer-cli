use std::fs;

use serde_json::Value;

use crate::support::test_support::{
    copy_recursive, devcontainer_command, repo_root, unique_temp_dir,
};

#[test]
fn outdated_supports_upstream_json_output_fixture() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-outdated-command");
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    copy_recursive(&fixture, &workspace);

    let output = devcontainer_command(None)
        .args([
            "outdated",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--output-format",
            "json",
        ])
        .output()
        .expect("outdated should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("json payload");
    assert_eq!(
        payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["current"],
        "1.0.4"
    );
    assert_eq!(
        payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["wanted"],
        "1.0.5"
    );
    assert_eq!(
        payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["latest"],
        "1.2.0"
    );
    assert_eq!(
        payload["features"]["ghcr.io/codspace/versioning/foo:0.3.1"]["latest"],
        "2.11.1"
    );
    assert!(payload["features"]
        .as_object()
        .expect("features object")
        .contains_key("ghcr.io/codspace/doesnotexist:0.1.2"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn outdated_supports_text_output_fixture() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-outdated-command");
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    copy_recursive(&fixture, &workspace);

    let output = devcontainer_command(None)
        .args([
            "outdated",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--output-format",
            "text",
        ])
        .output()
        .expect("outdated should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Current"));
    assert!(stdout.contains("Wanted"));
    assert!(stdout.contains("Latest"));
    assert!(stdout.contains("ghcr.io/devcontainers/features/git"));
    assert!(stdout.contains("ghcr.io/devcontainers/features/azure-cli"));
    assert!(!stdout.contains("mylocalfeature"));
    assert!(!stdout.contains("terraform"));

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn upgrade_matches_upstream_lockfile_fixture() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-upgrade-command");
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    copy_recursive(&fixture, &workspace);
    fs::copy(
        workspace.join("outdated.devcontainer-lock.json"),
        workspace.join(".devcontainer-lock.json"),
    )
    .expect("seed lockfile");

    let output = devcontainer_command(None)
        .args([
            "upgrade",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("upgrade should run");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(workspace.join(".devcontainer-lock.json")).expect("actual lockfile"),
        fs::read_to_string(workspace.join("upgraded.devcontainer-lock.json"))
            .expect("expected lockfile")
    );

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn upgrade_with_feature_updates_config_and_dry_run_lockfile() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-upgrade-feature");
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    copy_recursive(&fixture, &workspace);
    fs::copy(
        workspace.join("input.devcontainer.json"),
        workspace.join(".devcontainer.json"),
    )
    .expect("seed config");

    let output = devcontainer_command(None)
        .args([
            "upgrade",
            "--dry-run",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
            "--feature",
            "ghcr.io/codspace/versioning/foo",
            "--target-version",
            "2",
        ])
        .output()
        .expect("upgrade should run");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(workspace.join(".devcontainer.json")).expect("updated config"),
        fs::read_to_string(workspace.join("expected.devcontainer.json")).expect("expected config")
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("dry-run lockfile");
    assert_eq!(
        payload["features"]["ghcr.io/codspace/versioning/foo:2"]["version"],
        "2.11.1"
    );

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn upgrade_supports_upstream_dependson_lockfile_fixture() {
    let root = repo_root();
    let fixture = root
        .join("upstream")
        .join("src")
        .join("test")
        .join("container-features")
        .join("configs")
        .join("lockfile-dependson");
    let workspace = unique_temp_dir("devcontainer-cli-smoke");
    copy_recursive(&fixture, &workspace);

    let output = devcontainer_command(None)
        .args([
            "upgrade",
            "--dry-run",
            "--workspace-folder",
            workspace.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("upgrade should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("dry-run lockfile");
    assert_eq!(
        payload["features"]["ghcr.io/codspace/dependson/A:2"]["version"],
        "2.0.1"
    );
    assert_eq!(
        payload["features"]["ghcr.io/codspace/dependson/E:1"]["version"],
        "1.0.0"
    );
    assert_eq!(
        payload["features"]["https://github.com/codspace/tgz-features-with-dependson/releases/download/0.0.2/devcontainer-feature-A.tgz"]["version"],
        "2.0.1"
    );

    let _ = fs::remove_dir_all(workspace);
}
