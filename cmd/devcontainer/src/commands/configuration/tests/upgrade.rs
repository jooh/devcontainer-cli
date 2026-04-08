use std::fs;
use std::path::{Path, PathBuf};

use super::support::unique_temp_dir;
use crate::commands::configuration::upgrade::{
    build_outdated_payload, feature_id_without_version, lockfile_path, run_upgrade_lockfile,
};

#[test]
fn outdated_payload_reports_remote_feature_versions() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create root");
    fs::write(
        root.join(".devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/git:1.0\": \"latest\",\n    \"./local-feature\": {}\n  }\n}\n",
    )
    .expect("failed to write config");
    fs::write(
        root.join(".devcontainer-lock.json"),
        "{\n  \"features\": {\n    \"ghcr.io/devcontainers/features/git:1.0\": {\n      \"version\": \"1.0.4\",\n      \"resolved\": \"ghcr.io/devcontainers/features/git@sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6\",\n      \"integrity\": \"sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6\"\n    }\n  }\n}\n",
    )
    .expect("failed to write lockfile");

    let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
    let payload = build_outdated_payload(&args).expect("payload");

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
    assert!(payload["features"]["./local-feature"].is_null());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn upgrade_lockfile_uses_root_relative_lockfile_for_dotfile_configs() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("failed to create root");
    fs::write(
        root.join(".devcontainer.json"),
        "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/github-cli\": \"latest\"\n  }\n}\n",
    )
    .expect("failed to write config");

    let lockfile =
        run_upgrade_lockfile(&["--workspace-folder".to_string(), root.display().to_string()])
            .expect("lockfile payload");

    let lockfile_path = root.join(".devcontainer-lock.json");
    assert!(lockfile_path.is_file());
    assert_eq!(
        lockfile.features["ghcr.io/devcontainers/features/github-cli"].version,
        "1.0.9"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn feature_id_without_version_handles_tags_and_digests() {
    assert_eq!(
        feature_id_without_version("ghcr.io/devcontainers/features/git:1.0"),
        "ghcr.io/devcontainers/features/git"
    );
    assert_eq!(
        feature_id_without_version(
            "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c"
        ),
        "ghcr.io/devcontainers/features/git-lfs"
    );
}

#[test]
fn lockfile_path_matches_upstream_dotfile_rule() {
    assert_eq!(
        lockfile_path(Path::new("/tmp/workspace/.devcontainer.json")),
        PathBuf::from("/tmp/workspace/.devcontainer-lock.json")
    );
    assert_eq!(
        lockfile_path(Path::new("/tmp/workspace/.devcontainer/devcontainer.json")),
        PathBuf::from("/tmp/workspace/.devcontainer/devcontainer-lock.json")
    );
}
