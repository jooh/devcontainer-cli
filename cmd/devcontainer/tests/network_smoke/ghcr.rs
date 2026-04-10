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
    assert_eq!(
        payload["canonicalId"],
        "ghcr.io/codspace/features/ruby@sha256:4757b07cbfbfc09015d8a5b7fb1c44e83d85de4fae13e9f311f7b9ae9ae0c25c"
    );
    assert_eq!(
        payload["manifest"]["layers"][0]["mediaType"],
        "application/vnd.devcontainers.layer.v1+tar"
    );
    assert_eq!(
        payload["manifest"]["layers"][0]["digest"],
        "sha256:8f59630bd1ba6d9e78b485233a0280530b3d0a44338f472206090412ffbd3efb"
    );
}
