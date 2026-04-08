use std::env;
use std::path::PathBuf;

use serde_json::{json, Value};

pub(super) fn embedded_template_source_dir(reference: &str) -> Option<PathBuf> {
    let slug = collection_slug(reference)?;
    match slug.as_str() {
        "alpine" | "cpp" | "mytemplate" | "node-mongo" => Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(
                    "../../upstream/src/test/container-templates/example-templates-sets/simple/src",
                )
                .join(slug),
        ),
        _ => None,
    }
}

pub(super) fn published_feature_install_script(feature_id: &str) -> &'static str {
    match normalize_collection_reference(feature_id).as_str() {
        "ghcr.io/devcontainers/features/common-utils" => {
            r#"#!/bin/sh
set -eu

username="${USERNAME:-}"
if [ -n "$username" ] && [ "$username" != "none" ] && ! id -u "$username" >/dev/null 2>&1; then
    if command -v useradd >/dev/null 2>&1; then
        useradd -m "$username" >/dev/null 2>&1 || true
    elif command -v adduser >/dev/null 2>&1; then
        adduser -D "$username" >/dev/null 2>&1 || adduser --disabled-password --gecos "" "$username" >/dev/null 2>&1 || true
    fi
fi
"#
        }
        _ => {
            r#"#!/bin/sh
set -eu
"#
        }
    }
}

pub(super) fn published_feature_manifest(feature_id: &str) -> Option<Value> {
    let normalized = normalize_collection_reference(feature_id);
    let manifest = match normalized.as_str() {
        "ghcr.io/devcontainers/features/azure-cli" => Some(json!({
            "id": "azure-cli",
            "name": "Azure CLI",
            "version": "1.2.1",
            "options": { "version": { "type": "string", "default": "latest" } }
        })),
        "ghcr.io/devcontainers/features/common-utils" => Some(json!({
            "id": "common-utils",
            "name": "Common Utilities",
            "version": "2.5.4",
            "options": {
                "installZsh": { "type": "string", "default": "true" },
                "upgradePackages": { "type": "string", "default": "true" }
            }
        })),
        "ghcr.io/devcontainers/features/docker-from-docker" => Some(json!({
            "id": "docker-from-docker",
            "name": "Docker from Docker",
            "version": "2.12.4",
            "options": {
                "version": { "type": "string", "default": "latest" },
                "moby": { "type": "string", "default": "true" },
                "enableNonRootDocker": { "type": "string", "default": "true" }
            }
        })),
        "ghcr.io/devcontainers/features/github-cli" => Some(json!({
            "id": "github-cli",
            "name": "GitHub CLI",
            "version": "1.0.9",
            "options": {}
        })),
        _ => None,
    };
    if manifest.is_some() {
        return manifest;
    }

    let slug = collection_slug(&normalized)?;
    if !normalized.contains("/features/") {
        return None;
    }
    Some(json!({
        "id": slug,
        "name": humanize_collection_slug(&slug),
        "version": collection_reference_version(feature_id),
        "options": {},
    }))
}

pub(super) fn published_template_manifest(template_id: &str) -> Option<Value> {
    let normalized = normalize_collection_reference(template_id);
    let manifest = match normalized.as_str() {
        "ghcr.io/devcontainers/templates/docker-from-docker" => Some(json!({
            "id": "docker-from-docker",
            "name": "Docker from Docker",
            "description": "Create a dev container with Docker available inside the container.",
            "version": "1.0.0"
        })),
        _ => embedded_template_manifest(&normalized),
    };
    if manifest.is_some() {
        return manifest;
    }

    let slug = collection_slug(&normalized)?;
    if !normalized.contains("/templates/") {
        return None;
    }
    Some(json!({
        "id": slug,
        "name": humanize_collection_slug(&slug),
        "description": "",
        "version": collection_reference_version(template_id),
    }))
}

pub(super) fn normalize_collection_reference(reference: &str) -> String {
    if let Some(index) = reference.find('@') {
        return reference[..index].to_string();
    }
    let last_slash = reference.rfind('/').unwrap_or(0);
    if let Some(index) = reference.rfind(':').filter(|index| *index > last_slash) {
        return reference[..index].to_string();
    }
    reference.to_string()
}

pub(super) fn collection_slug(reference: &str) -> Option<String> {
    normalize_collection_reference(reference)
        .rsplit('/')
        .next()
        .map(|value| value.to_ascii_lowercase())
}

pub(super) fn collection_reference_version(reference: &str) -> String {
    let normalized = normalize_collection_reference(reference);
    if let Some(digest) = reference
        .strip_prefix(&normalized)
        .and_then(|suffix| suffix.strip_prefix('@'))
    {
        return digest.to_string();
    }
    if let Some(version) = reference
        .strip_prefix(&normalized)
        .and_then(|suffix| suffix.strip_prefix(':'))
    {
        return version.to_string();
    }
    "latest".to_string()
}

pub(super) fn humanize_collection_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn embedded_template_manifest(reference: &str) -> Option<Value> {
    match collection_slug(reference)?.as_str() {
        "alpine" => serde_json::from_str(include_str!(
            "../../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/alpine/devcontainer-template.json"
        ))
        .ok(),
        "cpp" => serde_json::from_str(include_str!(
            "../../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/cpp/devcontainer-template.json"
        ))
        .ok(),
        "mytemplate" => serde_json::from_str(include_str!(
            "../../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/mytemplate/devcontainer-template.json"
        ))
        .ok(),
        "node-mongo" => serde_json::from_str(include_str!(
            "../../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/node-mongo/devcontainer-template.json"
        ))
        .ok(),
        _ => None,
    }
}
