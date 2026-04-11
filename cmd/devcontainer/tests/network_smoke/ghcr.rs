//! Live GHCR smoke coverage for public published Feature flows.

use serde_json::Value;

use crate::support::test_support::devcontainer_command;

#[test]
#[ignore = "requires outbound internet access to ghcr.io"]
fn features_info_reads_live_ghcr_manifest() {
    let output = devcontainer_command(None)
        .env("DEVCONTAINER_ENABLE_LIVE_GHCR", "1")
        .args([
            "features",
            "info",
            "manifest",
            "ghcr.io/codspace/features/ruby:1.0.13",
        ])
        .output()
        .expect("features info should run");

    assert!(output.status.success(), "{output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).expect("feature info payload");
    assert!(payload["canonicalId"]
        .as_str()
        .expect("canonical id")
        .starts_with("ghcr.io/codspace/features/ruby@sha256:"));
    assert_eq!(payload["manifest"]["schemaVersion"], 2);
    assert_eq!(
        payload["manifest"]["mediaType"],
        "application/vnd.oci.image.manifest.v1+json"
    );
    assert_eq!(
        payload["manifest"]["layers"][0]["mediaType"],
        "application/vnd.devcontainers.layer.v1+tar"
    );
    assert!(payload["manifest"]["layers"][0]["digest"]
        .as_str()
        .expect("layer digest")
        .starts_with("sha256:"));
}

#[test]
#[ignore = "requires outbound internet access to ghcr.io"]
fn features_info_verbose_reads_live_ghcr_manifest_when_enabled() {
    let manifest_output = devcontainer_command(None)
        .env("DEVCONTAINER_ENABLE_LIVE_GHCR", "1")
        .args([
            "features",
            "info",
            "manifest",
            "ghcr.io/codspace/features/ruby:1.0.13",
        ])
        .output()
        .expect("manifest command should run");
    let verbose_output = devcontainer_command(None)
        .env("DEVCONTAINER_ENABLE_LIVE_GHCR", "1")
        .args([
            "features",
            "info",
            "verbose",
            "ghcr.io/codspace/features/ruby:1.0.13",
        ])
        .output()
        .expect("verbose command should run");

    assert!(manifest_output.status.success(), "{manifest_output:?}");
    assert!(verbose_output.status.success(), "{verbose_output:?}");
    let manifest_payload: Value =
        serde_json::from_slice(&manifest_output.stdout).expect("manifest payload");
    let verbose_payload: Value =
        serde_json::from_slice(&verbose_output.stdout).expect("verbose payload");
    assert_eq!(
        verbose_payload["canonicalId"],
        manifest_payload["canonicalId"]
    );
    assert_eq!(verbose_payload["manifest"], manifest_payload["manifest"]);
}
