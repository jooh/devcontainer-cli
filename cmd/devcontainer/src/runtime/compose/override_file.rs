use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::commands::common;

use super::super::context::{workspace_mount, ResolvedConfig};
use super::super::metadata::{serialized_container_metadata, split_mount_options};

static NEXT_OVERRIDE_FILE_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn compose_metadata_override_file(
    resolved: &ResolvedConfig,
    args: &[String],
    remote_workspace_folder: &str,
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
    if let Some(volume) = compose_workspace_volume(resolved, remote_workspace_folder) {
        content.push_str(&format!(
            "\n    volumes:\n      - '{}'\n",
            escape_compose_scalar(&volume)
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
    remote_workspace_folder: &str,
) -> Option<String> {
    let mount = workspace_mount(resolved, remote_workspace_folder);
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
