//! Configuration upgrade, lockfile, and outdated command helpers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{json, Map, Value};

use super::catalog::{
    build_feature_version_info, catalog_entry_for_version, exact_catalog_entry, latest_version,
    resolve_wanted_version,
};
use super::load::load_config;
use super::{FeatureReference, Lockfile, LockfileEntry};
use crate::commands::common;

pub(super) fn run_outdated(args: &[String]) -> ExitCode {
    match build_outdated_payload(args) {
        Ok(payload) => {
            let output_format = common::parse_option_value(args, "--output-format")
                .unwrap_or_else(|| "json".to_string());
            if output_format == "text" {
                println!("{}", render_outdated_text(&payload));
            } else {
                println!("{payload}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

pub(super) fn run_upgrade(args: &[String]) -> ExitCode {
    match run_upgrade_lockfile(args) {
        Ok(lockfile) => {
            if common::has_flag(args, "--dry-run") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lockfile).expect("lockfile json")
                );
            } else {
                println!(
                    "{}",
                    json!({
                        "outcome": "success",
                        "command": "upgrade",
                        "lockfile": lockfile,
                    })
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

pub(super) fn ensure_native_lockfile(
    args: &[String],
    config_file: &Path,
    configuration: &Value,
) -> Result<(), String> {
    let wants_lockfile = common::has_flag(args, "--experimental-lockfile")
        || common::has_flag(args, "--experimental-frozen-lockfile");
    if !wants_lockfile {
        return Ok(());
    }

    let generated = generate_lockfile(configuration)?;
    let path = lockfile_path(config_file);
    if common::has_flag(args, "--experimental-frozen-lockfile") {
        let existing = read_lockfile(path.clone())?;
        if existing.as_ref() != Some(&generated) {
            return Err(format!(
                "Lockfile at {} is out of date for the current feature configuration",
                path.display()
            ));
        }
    }
    if common::has_flag(args, "--experimental-lockfile") {
        fs::write(
            &path,
            serde_json::to_string_pretty(&generated).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn build_outdated_payload(args: &[String]) -> Result<Value, String> {
    let loaded = load_config(args)?;
    let lockfile = read_lockfile(lockfile_path(&loaded.config_file))?;
    let features = loaded
        .configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut payload_features = Map::new();
    for feature_id in features.keys() {
        let Some(reference) = parse_feature_reference(feature_id) else {
            continue;
        };

        let Some(feature_info) = build_feature_version_info(&reference, lockfile.as_ref()) else {
            continue;
        };
        payload_features.insert(feature_id.clone(), feature_info);
    }

    Ok(json!({
        "features": payload_features,
    }))
}

pub(super) fn run_upgrade_lockfile(args: &[String]) -> Result<Lockfile, String> {
    validate_upgrade_options(args)?;

    let mut loaded = load_config(args)?;
    if let (Some(feature), Some(target_version)) = (
        common::parse_option_value(args, "--feature"),
        common::parse_option_value(args, "--target-version"),
    ) {
        update_feature_version_in_config(
            &loaded.config_file,
            &loaded.raw_text,
            &loaded.configuration,
            &feature,
            &target_version,
        )?;
        loaded = load_config(args)?;
    }

    let generated = generate_lockfile(&loaded.configuration)?;
    if !common::has_flag(args, "--dry-run") {
        let lockfile_path = lockfile_path(&loaded.config_file);
        fs::write(
            &lockfile_path,
            serde_json::to_string_pretty(&generated).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(generated)
}

fn validate_upgrade_options(args: &[String]) -> Result<(), String> {
    let feature = common::parse_option_value(args, "--feature");
    let target_version = common::parse_option_value(args, "--target-version");

    if feature.is_some() != target_version.is_some() {
        return Err(
            "The '--target-version' and '--feature' flag must be used together.".to_string(),
        );
    }

    if let Some(version) = target_version {
        if !version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
            || version.is_empty()
        {
            return Err(format!(
                "Invalid version '{version}'. Must be in the form of 'x', 'x.y', or 'x.y.z'"
            ));
        }
    }

    Ok(())
}

fn generate_lockfile(configuration: &Value) -> Result<Lockfile, String> {
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut resolved = std::collections::BTreeMap::new();
    for feature_id in features.keys() {
        let Some(reference) = parse_feature_reference(feature_id) else {
            continue;
        };

        let (lockfile_key, entry) = generate_lockfile_entry(&reference).ok_or_else(|| {
            format!("Unsupported feature for native lockfile generation: {feature_id}")
        })?;
        resolved.insert(lockfile_key, entry);
    }

    Ok(Lockfile { features: resolved })
}

fn generate_lockfile_entry(feature: &FeatureReference) -> Option<(String, LockfileEntry)> {
    if feature.digest.is_some() {
        return exact_catalog_entry(&feature.original).map(|entry| {
            (
                feature.original.clone(),
                LockfileEntry {
                    version: entry.version.clone(),
                    resolved: entry.resolved.clone(),
                    integrity: entry.integrity.clone(),
                    depends_on: entry.depends_on.clone(),
                },
            )
        });
    }

    let version = if let Some(tag) = feature.tag.as_deref() {
        if tag == "latest" {
            latest_version(&feature.base)?
        } else if tag.matches('.').count() == 2 {
            tag.to_string()
        } else {
            resolve_wanted_version(feature, None)?
        }
    } else {
        latest_version(&feature.base)?
    };

    let entry = catalog_entry_for_version(&feature.base, &version)?;
    Some((
        feature.original.clone(),
        LockfileEntry {
            version,
            resolved: entry.resolved.clone(),
            integrity: entry.integrity.clone(),
            depends_on: entry.depends_on.clone(),
        },
    ))
}

pub(super) fn lockfile_path(config_file: &Path) -> PathBuf {
    let file_name = config_file
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("devcontainer.json");
    let lockfile_name = if file_name.starts_with('.') {
        ".devcontainer-lock.json"
    } else {
        "devcontainer-lock.json"
    };
    config_file
        .parent()
        .unwrap_or(config_file)
        .join(lockfile_name)
}

fn read_lockfile(path: PathBuf) -> Result<Option<Lockfile>, String> {
    match fs::read_to_string(path) {
        Ok(contents) if contents.trim().is_empty() => Ok(None),
        Ok(contents) => serde_json::from_str(&contents)
            .map(Some)
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn update_feature_version_in_config(
    config_path: &Path,
    raw_text: &str,
    configuration: &Value,
    target_feature: &str,
    target_version: &str,
) -> Result<(), String> {
    let target_base = feature_id_without_version(target_feature);
    let current_key = configuration
        .get("features")
        .and_then(Value::as_object)
        .and_then(|entries| {
            entries
                .keys()
                .find(|feature_id| feature_id_without_version(feature_id) == target_base)
        })
        .cloned();

    let Some(current_key) = current_key else {
        return Ok(());
    };

    let updated = raw_text.replace(&current_key, &format!("{target_base}:{target_version}"));
    if updated != raw_text {
        fs::write(config_path, updated).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn render_outdated_text(payload: &Value) -> String {
    let mut rows = vec![vec![
        "Feature".to_string(),
        "Current".to_string(),
        "Wanted".to_string(),
        "Latest".to_string(),
    ]];

    if let Some(features) = payload.get("features").and_then(Value::as_object) {
        for (key, value) in features {
            rows.push(vec![
                feature_id_without_version(key),
                cell(value.get("current")),
                cell(value.get("wanted")),
                cell(value.get("latest")),
            ]);
        }
    }

    let widths = (0..rows[0].len())
        .map(|index| rows.iter().map(|row| row[index].len()).max().unwrap_or(0))
        .collect::<Vec<_>>();

    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .enumerate()
                .map(|(index, cell)| format!("{cell:width$}", width = widths[index]))
                .collect::<Vec<_>>()
                .join("  ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cell(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("-").to_string()
}

fn parse_feature_reference(feature_id: &str) -> Option<FeatureReference> {
    if !feature_id.starts_with("ghcr.io/")
        && !feature_id.starts_with("https://")
        && !feature_id.starts_with("http://")
    {
        return None;
    }

    let base = feature_id_without_version(feature_id);
    let suffix = feature_id.strip_prefix(&base)?;
    if suffix.is_empty() {
        return Some(FeatureReference {
            original: feature_id.to_string(),
            base,
            tag: None,
            digest: None,
        });
    }

    if let Some(digest) = suffix.strip_prefix('@') {
        return Some(FeatureReference {
            original: feature_id.to_string(),
            base,
            tag: None,
            digest: Some(digest.to_string()),
        });
    }

    suffix.strip_prefix(':').map(|tag| FeatureReference {
        original: feature_id.to_string(),
        base,
        tag: Some(tag.to_string()),
        digest: None,
    })
}

pub(super) fn feature_id_without_version(feature_id: &str) -> String {
    if let Some(index) = feature_id.find("@sha256:") {
        return feature_id[..index].to_string();
    }

    let last_slash = feature_id.rfind('/').unwrap_or(0);
    let last_colon = feature_id.rfind(':');
    let last_at = feature_id.rfind('@');
    let delimiter = match (last_colon, last_at) {
        (Some(colon), Some(at)) => Some(colon.max(at)),
        (Some(colon), None) => Some(colon),
        (None, Some(at)) => Some(at),
        (None, None) => None,
    };

    match delimiter.filter(|index| *index > last_slash) {
        Some(index) => feature_id[..index].to_string(),
        None => feature_id.to_string(),
    }
}
