use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::commands::common;

use super::super::context::{derived_workspace_mount, workspace_mount_for_args, ResolvedConfig};
use super::super::metadata::{serialized_container_metadata, split_mount_options};

static NEXT_OVERRIDE_FILE_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn compose_metadata_override_file(
    resolved: &ResolvedConfig,
    args: &[String],
    remote_workspace_folder: &str,
    image_name: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let metadata = serialized_container_metadata(&resolved.configuration, remote_workspace_folder)?;
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
    if let Some(volume) = compose_workspace_volume(resolved, args, remote_workspace_folder) {
        content.push_str(&format!(
            "\n    volumes:\n      - '{}'\n",
            escape_compose_scalar(&volume)
        ));
    }
    for volume in compose_additional_volumes(&resolved.configuration) {
        if !content.contains("\n    volumes:\n") {
            content.push_str("\n    volumes:\n");
        }
        content.push_str(&format!("      - '{}'\n", escape_compose_scalar(&volume)));
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
        .or_else(|| resolved.configuration.get("remoteUser"))
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

    let override_file = unique_override_file_path();
    std::fs::write(&override_file, content).map_err(|error| error.to_string())?;
    Ok(Some(override_file))
}

fn escape_compose_label(label: &str) -> String {
    label.replace('\'', "''").replace('$', "$$")
}

fn escape_compose_scalar(value: &str) -> String {
    value.replace('\'', "''")
}

fn compose_workspace_volume(
    resolved: &ResolvedConfig,
    args: &[String],
    remote_workspace_folder: &str,
) -> Option<String> {
    if resolved.configuration.get("workspaceMount").is_none() {
        return derived_workspace_mount(&resolved.workspace_folder, args).map(|derived| {
            format!(
                "{}:{remote_workspace_folder}",
                derived.host_mount_folder.display()
            )
        });
    }
    let mount = workspace_mount_for_args(resolved, remote_workspace_folder, args);
    let mut mount_type = None;
    let mut source = None;
    let mut target = None;
    let mut read_only = false;
    for option in split_mount_options(&mount) {
        if option == "readonly" || option == "ro" {
            read_only = true;
            continue;
        }
        if let Some(value) = option.strip_prefix("type=") {
            mount_type = Some(value.trim_matches('"').to_string());
        } else if let Some(value) = option
            .strip_prefix("source=")
            .or_else(|| option.strip_prefix("src="))
        {
            source = Some(value.trim_matches('"').to_string());
        } else if let Some(value) = option
            .strip_prefix("target=")
            .or_else(|| option.strip_prefix("destination="))
            .or_else(|| option.strip_prefix("dst="))
        {
            target = Some(value.trim_matches('"').to_string());
        }
    }

    if mount_type.as_deref().unwrap_or("bind") != "bind" {
        return None;
    }

    let mut volume = format!("{}:{}", source?, target?);
    if read_only {
        volume.push_str(":ro");
    }
    Some(volume)
}

fn compose_additional_volumes(configuration: &Value) -> Vec<String> {
    configuration
        .get("mounts")
        .and_then(Value::as_array)
        .map(|mounts| mounts.iter().filter_map(compose_mount_scalar).collect())
        .unwrap_or_default()
}

fn compose_mount_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(entries) => {
            let mut parts = Vec::new();
            for key in ["type", "source", "target", "external"] {
                let Some(value) = entries.get(key) else {
                    continue;
                };
                let text = match value {
                    Value::Bool(boolean) => boolean.to_string(),
                    Value::Number(number) => number.to_string(),
                    Value::String(text) => text.clone(),
                    _ => continue,
                };
                parts.push(format!("{key}={text}"));
            }
            (!parts.is_empty()).then(|| parts.join(","))
        }
        _ => None,
    }
}

fn compose_environment(configuration: &Value) -> Option<Vec<(String, String)>> {
    let env = configuration
        .get("containerEnv")
        .and_then(Value::as_object)?
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|text| (key.clone(), text.to_string())))
        .collect::<Vec<_>>();
    (!env.is_empty()).then_some(env)
}

fn unique_override_file_path() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let unique_id = NEXT_OVERRIDE_FILE_ID.fetch_add(1, Ordering::Relaxed);
    env::temp_dir().join(format!(
        "devcontainer-compose-override-{}-{suffix}-{unique_id}.yml",
        std::process::id()
    ))
}
