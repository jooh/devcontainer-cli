use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::commands::common;

use super::super::context::{derived_workspace_mount, workspace_mount_for_args, ResolvedConfig};
use super::super::metadata::{serialized_container_metadata, split_mount_options};

static NEXT_OVERRIDE_FILE_ID: AtomicU64 = AtomicU64::new(0);

enum ComposeVolumeEntry {
    Short(String),
    Long(ComposeMountDefinition),
}

struct ComposeMountDefinition {
    mount_type: String,
    source: Option<String>,
    target: String,
    read_only: bool,
    external: Option<bool>,
}

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
) -> Option<ComposeVolumeEntry> {
    if resolved.configuration.get("workspaceMount").is_none() {
        return derived_workspace_mount(&resolved.workspace_folder, args).map(|derived| {
            ComposeVolumeEntry::Short(format!(
                "{}:{}",
                derived.host_mount_folder.display(),
                derived.container_mount_folder
            ))
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
    Some(ComposeVolumeEntry::Short(volume))
}

fn compose_additional_volumes(
    resolved: &ResolvedConfig,
    args: &[String],
) -> Vec<ComposeVolumeEntry> {
    let mut volumes: Vec<ComposeVolumeEntry> = resolved
        .configuration
        .get("mounts")
        .and_then(Value::as_array)
        .map(|mounts| mounts.iter().filter_map(compose_mount_definition).collect())
        .unwrap_or_default();
    if resolved.configuration.get("workspaceMount").is_none() {
        if let Some(derived) = derived_workspace_mount(&resolved.workspace_folder, args) {
            volumes.extend(
                derived
                    .additional_mounts
                    .iter()
                    .filter_map(|mount| compose_mount_definition_from_str(mount))
                    .map(ComposeVolumeEntry::Long),
            );
        }
    }
    volumes
}

fn compose_mount_definition(value: &Value) -> Option<ComposeVolumeEntry> {
    match value {
        Value::String(text) => {
            compose_mount_definition_from_str(text).map(ComposeVolumeEntry::Long)
        }
        Value::Object(entries) => {
            let mount_type = entries
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("bind")
                .to_string();
            let source = entries
                .get("source")
                .or_else(|| entries.get("src"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let target = entries
                .get("target")
                .or_else(|| entries.get("destination"))
                .or_else(|| entries.get("dst"))
                .and_then(Value::as_str)?
                .to_string();
            let read_only = entries
                .get("readonly")
                .or_else(|| entries.get("readOnly"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let external = entries.get("external").and_then(Value::as_bool);
            Some(ComposeVolumeEntry::Long(ComposeMountDefinition {
                mount_type,
                source,
                target,
                read_only,
                external,
            }))
        }
        _ => None,
    }
}

fn compose_mount_definition_from_str(mount: &str) -> Option<ComposeMountDefinition> {
    let mut mount_type = "bind".to_string();
    let mut source = None;
    let mut target = None;
    let mut read_only = false;
    let mut external = None;
    for option in split_mount_options(mount) {
        if option == "readonly" || option == "ro" {
            read_only = true;
            continue;
        }
        if let Some(value) = option.strip_prefix("type=") {
            mount_type = value.trim_matches('"').to_string();
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
        } else if let Some(value) = option.strip_prefix("external=") {
            external = match value.trim_matches('"') {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        }
    }

    Some(ComposeMountDefinition {
        mount_type,
        source,
        target: target?,
        read_only,
        external,
    })
}

fn render_compose_volume_entry(entry: &ComposeVolumeEntry) -> String {
    match entry {
        ComposeVolumeEntry::Short(volume) => {
            format!("      - '{}'\n", escape_compose_scalar(volume))
        }
        ComposeVolumeEntry::Long(definition) => {
            let mut rendered = format!(
                "      - type: '{}'\n",
                escape_compose_scalar(&definition.mount_type)
            );
            if let Some(source) = &definition.source {
                rendered.push_str(&format!(
                    "        source: '{}'\n",
                    escape_compose_scalar(source)
                ));
            }
            rendered.push_str(&format!(
                "        target: '{}'\n",
                escape_compose_scalar(&definition.target)
            ));
            if definition.read_only {
                rendered.push_str("        read_only: true\n");
            }
            if let Some(external) = definition.external {
                rendered.push_str("        volume:\n");
                rendered.push_str(&format!(
                    "          external: {}\n",
                    if external { "true" } else { "false" }
                ));
            }
            rendered
        }
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
