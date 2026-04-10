//! Feature publishing command helpers for collection workflows.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::commands::common;

pub(super) fn publish_collection_target_to_oci(
    target: &Path,
    manifest_name: &str,
    prefix: &str,
    command: &str,
    args: &[String],
) -> Result<Value, String> {
    let manifest = common::parse_manifest(target, manifest_name)?;
    let archive = common::package_collection_target(target, manifest_name, prefix)?;
    let registry =
        common::parse_option_value(args, "--registry").unwrap_or_else(|| "ghcr.io".to_string());
    let namespace = common::parse_option_value(args, "--namespace");
    let output_dir = common::parse_option_value(args, "--output-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            target
                .parent()
                .unwrap_or(target)
                .join(format!("{prefix}-oci-layout"))
        });
    let resource = namespace.as_ref().and_then(|namespace| {
        manifest
            .get("id")
            .and_then(Value::as_str)
            .map(|id| format!("{registry}/{namespace}/{id}"))
    });
    let digest = write_oci_layout(&output_dir, &archive, &manifest, resource.as_deref())?;
    Ok(json!({
        "outcome": "success",
        "command": command,
        "archive": archive,
        "published": true,
        "layout": output_dir,
        "digest": digest,
        "mode": "local-oci-layout",
        "registry": registry,
        "namespace": namespace,
        "resource": resource,
    }))
}

fn write_oci_layout(
    output_dir: &Path,
    archive: &Path,
    metadata: &Value,
    resource: Option<&str>,
) -> Result<String, String> {
    fs::create_dir_all(output_dir.join("blobs").join("sha256"))
        .map_err(|error| error.to_string())?;
    fs::write(
        output_dir.join("oci-layout"),
        "{\n  \"imageLayoutVersion\": \"1.0.0\"\n}\n",
    )
    .map_err(|error| error.to_string())?;

    let config_bytes = b"{}".to_vec();
    let config_digest = sha256_digest(&config_bytes);
    fs::write(
        output_dir.join("blobs").join("sha256").join(&config_digest),
        &config_bytes,
    )
    .map_err(|error| error.to_string())?;

    let layer_bytes = fs::read(archive).map_err(|error| error.to_string())?;
    let layer_digest = sha256_digest(&layer_bytes);
    fs::write(
        output_dir.join("blobs").join("sha256").join(&layer_digest),
        &layer_bytes,
    )
    .map_err(|error| error.to_string())?;

    let mut annotations = json!({
        "dev.containers.metadata": serde_json::to_string(metadata).map_err(|error| error.to_string())?,
    });
    if let Some(resource) = resource {
        annotations
            .as_object_mut()
            .expect("annotations object")
            .insert(
                "org.opencontainers.image.ref.name".to_string(),
                Value::String(resource.to_string()),
            );
    }
    let manifest_json = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.empty.v1+json",
            "digest": format!("sha256:{config_digest}"),
            "size": config_bytes.len(),
        },
        "layers": [{
            "mediaType": "application/vnd.devcontainers.layer.v1+tar+gzip",
            "digest": format!("sha256:{layer_digest}"),
            "size": layer_bytes.len(),
        }],
        "annotations": annotations,
    });
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest_json).map_err(|error| error.to_string())?;
    let manifest_digest = sha256_digest(&manifest_bytes);
    fs::write(
        output_dir
            .join("blobs")
            .join("sha256")
            .join(&manifest_digest),
        &manifest_bytes,
    )
    .map_err(|error| error.to_string())?;

    let ref_name = metadata
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("latest");
    fs::write(
        output_dir.join("index.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": 2,
            "manifests": [{
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": format!("sha256:{manifest_digest}"),
                "size": manifest_bytes.len(),
                "annotations": {
                    "org.opencontainers.image.ref.name": ref_name,
                }
            }]
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(format!("sha256:{manifest_digest}"))
}

fn sha256_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
