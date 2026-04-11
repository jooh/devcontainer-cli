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
