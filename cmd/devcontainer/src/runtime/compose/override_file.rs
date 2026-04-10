//! Compose override-file generation for native runtime flows.

#[path = "override_mounts.rs"]
mod override_mounts;
#[path = "override_yaml.rs"]
mod override_yaml;

use std::path::PathBuf;

use serde_json::Value;

use crate::commands::common;

use super::super::container;
use super::super::context::ResolvedConfig;
use super::super::metadata::serialized_container_metadata;
use super::super::paths::unique_temp_path;
use override_mounts::{compose_additional_volumes, compose_environment, compose_workspace_volume};
use override_yaml::{escape_compose_label, escape_compose_scalar, render_compose_volume_entry};

pub(super) fn compose_metadata_override_file(
    resolved: &ResolvedConfig,
    args: &[String],
    remote_workspace_folder: &str,
    image_name: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let metadata = serialized_container_metadata(
        &resolved.configuration,
        remote_workspace_folder,
        common::runtime_options(args).omit_config_remote_env_from_metadata,
    )?;
    let mut labels = vec![
        format!(
            "devcontainer.local_folder={}",
            resolved.workspace_folder.display()
        ),
        format!(
            "devcontainer.config_file={}",
            resolved.config_file.display()
        ),
        format!("devcontainer.metadata={metadata}"),
    ];
    labels.extend(common::parse_option_values(args, "--id-label"));
    if labels.is_empty() {
        return Ok(None);
    }

    let mut content = String::from("services:\n");
    content.push_str(&format!(
        "  '{}':\n    labels:{}\n",
        resolved
            .configuration
            .get("service")
            .and_then(Value::as_str)
            .ok_or_else(|| "Compose configuration must define service".to_string())?,
        labels
            .iter()
            .map(|label| format!("\n      - '{}'", escape_compose_label(label)))
            .collect::<String>()
    ));
    if let Some(image_name) = image_name {
        content.push_str(&format!(
            "    image: '{}'\n",
            escape_compose_scalar(image_name)
        ));
    }
    let mut volumes = Vec::new();
    if let Some(volume) = compose_workspace_volume(resolved, args, remote_workspace_folder) {
        volumes.push(volume);
    }
    volumes.extend(compose_additional_volumes(resolved, args));
    if !volumes.is_empty() {
        content.push_str("\n    volumes:\n");
        for volume in volumes {
            content.push_str(&render_compose_volume_entry(&volume));
        }
    }
    if let Some(environment) = compose_environment(&resolved.configuration) {
        content.push_str("    environment:\n");
        for (key, value) in environment {
            content.push_str(&format!(
                "      {}: '{}'\n",
                key,
                escape_compose_scalar(&value)
            ));
        }
    }
    if let Some(user) = resolved
        .configuration
        .get("containerUser")
        .and_then(Value::as_str)
    {
        content.push_str(&format!("    user: '{}'\n", escape_compose_scalar(user)));
    }
    if resolved
        .configuration
        .get("privileged")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        content.push_str("    privileged: true\n");
    }
    if resolved
        .configuration
        .get("init")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        content.push_str("    init: true\n");
    }
    if let Some(cap_add) = resolved
        .configuration
        .get("capAdd")
        .and_then(Value::as_array)
    {
        content.push_str("    cap_add:\n");
        for capability in cap_add.iter().filter_map(Value::as_str) {
            content.push_str(&format!(
                "      - '{}'\n",
                escape_compose_scalar(capability)
            ));
        }
    }
    if let Some(security_opt) = resolved
        .configuration
        .get("securityOpt")
        .and_then(Value::as_array)
    {
        content.push_str("    security_opt:\n");
        for option in security_opt.iter().filter_map(Value::as_str) {
            content.push_str(&format!("      - '{}'\n", escape_compose_scalar(option)));
        }
    }
    if let Some(entrypoint) = resolved
        .configuration
        .get("entrypoint")
        .and_then(Value::as_str)
    {
        content.push_str(&format!(
            "    entrypoint: '{}'\n",
            escape_compose_scalar(entrypoint)
        ));
    }
    if container::should_add_gpu_capability(&resolved.configuration, args)? {
        content.push_str(
            "    deploy:\n      resources:\n        reservations:\n          devices:\n            - capabilities: [gpu]\n",
        );
    }

    let override_file = unique_override_file_path();
    std::fs::write(&override_file, content).map_err(|error| error.to_string())?;
    Ok(Some(override_file))
}

fn unique_override_file_path() -> PathBuf {
    unique_temp_path("devcontainer-compose-override", Some("yml"))
}
