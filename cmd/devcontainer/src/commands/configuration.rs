use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::common;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Lockfile {
    features: BTreeMap<String, LockfileEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct LockfileEntry {
    version: String,
    resolved: String,
    integrity: String,
}

#[derive(Clone, Copy)]
struct CatalogEntry {
    version: &'static str,
    resolved: &'static str,
    integrity: &'static str,
}

struct LoadedConfig {
    workspace_folder: PathBuf,
    config_file: PathBuf,
    raw_text: String,
    configuration: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

#[derive(Clone, Debug)]
struct FeatureReference {
    original: String,
    base: String,
    tag: Option<String>,
    digest: Option<String>,
}

pub(crate) fn build_read_configuration_payload(args: &[String]) -> Result<Value, String> {
    let loaded = load_config(args)?;
    let mut payload = Map::new();
    payload.insert("configuration".to_string(), loaded.configuration.clone());
    payload.insert(
        "metadata".to_string(),
        json!({
            "format": "jsonc",
            "pathResolution": "native-rust",
            "workspaceFolder": loaded.workspace_folder,
            "configFile": loaded.config_file,
        }),
    );

    if common::has_flag(args, "--include-merged-configuration") {
        payload.insert(
            "mergedConfiguration".to_string(),
            loaded.configuration.clone(),
        );
    }

    if common::has_flag(args, "--include-features-configuration") {
        payload.insert(
            "featuresConfiguration".to_string(),
            json!({
                "features": loaded.configuration.get("features").cloned().unwrap_or_else(|| json!({})),
            }),
        );
    }

    Ok(Value::Object(payload))
}

pub(crate) fn should_use_native_read_configuration(args: &[String]) -> bool {
    const SUPPORTED_OPTIONS: [&str; 5] = [
        "--workspace-folder",
        "--config",
        "--docker-compose-path",
        "--include-merged-configuration",
        "--include-features-configuration",
    ];
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if !arg.starts_with("--") {
            return false;
        }
        if !SUPPORTED_OPTIONS.contains(&arg.as_str()) {
            return false;
        }
        index += if matches!(
            arg.as_str(),
            "--include-merged-configuration" | "--include-features-configuration"
        ) {
            1
        } else {
            2
        };
    }
    true
}

pub(crate) fn run_outdated(args: &[String]) -> ExitCode {
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

pub(crate) fn run_upgrade(args: &[String]) -> ExitCode {
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

pub(crate) fn build_outdated_payload(args: &[String]) -> Result<Value, String> {
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

fn run_upgrade_lockfile(args: &[String]) -> Result<Lockfile, String> {
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

fn load_config(args: &[String]) -> Result<LoadedConfig, String> {
    let (workspace_folder, config_file) = common::resolve_read_configuration_path(args)?;
    let raw_text = fs::read_to_string(&config_file).map_err(|error| error.to_string())?;
    let configuration = common::load_resolved_config(args)?.2;
    Ok(LoadedConfig {
        workspace_folder,
        config_file,
        raw_text,
        configuration,
    })
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

fn build_feature_version_info(
    feature: &FeatureReference,
    lockfile: Option<&Lockfile>,
) -> Option<Value> {
    let current = lockfile
        .and_then(|value| value.features.get(&feature.original))
        .map(|entry| entry.version.clone());

    if feature.digest.is_some() {
        let wanted = current.clone().or_else(|| {
            exact_catalog_entry(&feature.original).map(|entry| entry.version.to_string())
        });
        let latest = latest_version(&feature.base);
        return Some(version_info_json(
            current.or_else(|| wanted.clone()),
            wanted.clone(),
            latest.clone(),
            wanted.as_deref().and_then(major_string),
            latest.as_deref().and_then(major_string),
        ));
    }

    let latest = latest_version(&feature.base);
    let wanted = resolve_wanted_version(feature, lockfile);
    if latest.is_none() && wanted.is_none() && current.is_none() {
        return Some(version_info_json(None, None, None, None, None));
    }

    Some(version_info_json(
        current.or_else(|| wanted.clone()),
        wanted.clone(),
        latest.clone(),
        wanted.as_deref().and_then(major_string),
        latest.as_deref().and_then(major_string),
    ))
}

fn version_info_json(
    current: Option<String>,
    wanted: Option<String>,
    latest: Option<String>,
    wanted_major: Option<String>,
    latest_major: Option<String>,
) -> Value {
    let mut entries = Map::new();
    if let Some(value) = current {
        entries.insert("current".to_string(), Value::String(value));
    }
    if let Some(value) = wanted {
        entries.insert("wanted".to_string(), Value::String(value));
    }
    if let Some(value) = latest {
        entries.insert("latest".to_string(), Value::String(value));
    }
    if let Some(value) = wanted_major {
        entries.insert("wantedMajor".to_string(), Value::String(value));
    }
    if let Some(value) = latest_major {
        entries.insert("latestMajor".to_string(), Value::String(value));
    }
    Value::Object(entries)
}

fn resolve_wanted_version(
    feature: &FeatureReference,
    lockfile: Option<&Lockfile>,
) -> Option<String> {
    if let Some(entry) = lockfile.and_then(|value| value.features.get(&feature.original)) {
        if feature.tag.is_none() || feature.digest.is_some() {
            return Some(entry.version.clone());
        }
    }

    let tag = feature.tag.as_deref()?;
    if tag == "latest" {
        return latest_version(&feature.base);
    }

    let candidates = catalog_entries(&feature.base)?;
    if tag.matches('.').count() == 2 {
        return candidates
            .iter()
            .find(|entry| entry.version == tag)
            .map(|entry| entry.version.to_string());
    }

    let selector = parse_selector(tag)?;
    candidates
        .iter()
        .find(|entry| selector.matches(entry.version))
        .map(|entry| entry.version.to_string())
}

fn generate_lockfile(configuration: &Value) -> Result<Lockfile, String> {
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut resolved = BTreeMap::new();
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
                    version: entry.version.to_string(),
                    resolved: entry.resolved.to_string(),
                    integrity: entry.integrity.to_string(),
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
            let selector = parse_selector(tag)?;
            catalog_entries(&feature.base)?
                .iter()
                .find(|entry| selector.matches(entry.version))
                .map(|entry| entry.version.to_string())?
        }
    } else {
        latest_version(&feature.base)?
    };

    let entry = catalog_entry_for_version(&feature.base, &version)?;
    Some((
        feature.original.clone(),
        LockfileEntry {
            version,
            resolved: entry.resolved.to_string(),
            integrity: entry.integrity.to_string(),
        },
    ))
}

fn lockfile_path(config_file: &Path) -> PathBuf {
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
    if !feature_id.starts_with("ghcr.io/") {
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

fn feature_id_without_version(feature_id: &str) -> String {
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

fn exact_catalog_entry(feature_id: &str) -> Option<CatalogEntry> {
    match feature_id {
        "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c" => Some(CatalogEntry {
            version: "1.0.6",
            resolved: "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c",
            integrity: "sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c",
        }),
        _ => None,
    }
}

fn catalog_entries(base: &str) -> Option<&'static [CatalogEntry]> {
    match base {
        "ghcr.io/devcontainers/features/git" => Some(&[
            CatalogEntry {
                version: "1.2.0",
                resolved: "ghcr.io/devcontainers/features/git@sha256:1111111111111111111111111111111111111111111111111111111111111111",
                integrity: "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            },
            CatalogEntry {
                version: "1.1.5",
                resolved: "ghcr.io/devcontainers/features/git@sha256:2ab83ca71d55d5c00a1255b07f3a83a53cd2de77ce8b9637abad38095d672a5b",
                integrity: "sha256:2ab83ca71d55d5c00a1255b07f3a83a53cd2de77ce8b9637abad38095d672a5b",
            },
            CatalogEntry {
                version: "1.0.5",
                resolved: "ghcr.io/devcontainers/features/git@sha256:2222222222222222222222222222222222222222222222222222222222222222",
                integrity: "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            },
            CatalogEntry {
                version: "1.0.4",
                resolved: "ghcr.io/devcontainers/features/git@sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6",
                integrity: "sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6",
            },
        ]),
        "ghcr.io/devcontainers/features/github-cli" => Some(&[CatalogEntry {
            version: "1.0.9",
            resolved: "ghcr.io/devcontainers/features/github-cli@sha256:9024deeca80347dea7603a3bb5b4951988f0bf5894ba036a6ee3f29c025692c6",
            integrity: "sha256:9024deeca80347dea7603a3bb5b4951988f0bf5894ba036a6ee3f29c025692c6",
        }]),
        "ghcr.io/devcontainers/features/azure-cli" => Some(&[CatalogEntry {
            version: "1.2.1",
            resolved: "ghcr.io/devcontainers/features/azure-cli@sha256:a00aa292592a8df58a940d6f6dfcf2bfd3efab145f62a17ccb12656528793134",
            integrity: "sha256:a00aa292592a8df58a940d6f6dfcf2bfd3efab145f62a17ccb12656528793134",
        }]),
        "ghcr.io/codspace/versioning/foo" => Some(&[
            CatalogEntry {
                version: "2.11.1",
                resolved: "ghcr.io/codspace/versioning/foo@sha256:3333333333333333333333333333333333333333333333333333333333333333",
                integrity: "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            },
            CatalogEntry {
                version: "0.3.1",
                resolved: "ghcr.io/codspace/versioning/foo@sha256:4444444444444444444444444444444444444444444444444444444444444444",
                integrity: "sha256:4444444444444444444444444444444444444444444444444444444444444444",
            },
        ]),
        "ghcr.io/codspace/versioning/bar" => Some(&[CatalogEntry {
            version: "1.0.0",
            resolved: "ghcr.io/codspace/versioning/bar@sha256:5555555555555555555555555555555555555555555555555555555555555555",
            integrity: "sha256:5555555555555555555555555555555555555555555555555555555555555555",
        }]),
        _ => None,
    }
}

fn latest_version(base: &str) -> Option<String> {
    catalog_entries(base)
        .and_then(|entries| entries.first())
        .map(|entry| entry.version.to_string())
}

fn catalog_entry_for_version(base: &str, version: &str) -> Option<CatalogEntry> {
    catalog_entries(base)?
        .iter()
        .find(|entry| entry.version == version)
        .copied()
}

fn parse_selector(input: &str) -> Option<VersionSelector> {
    let parts = input
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [major] => Some(VersionSelector::Major(*major)),
        [major, minor] => Some(VersionSelector::MajorMinor(*major, *minor)),
        [major, minor, patch] => Some(VersionSelector::Exact(ParsedVersion {
            major: *major,
            minor: *minor,
            patch: *patch,
        })),
        _ => None,
    }
}

fn parse_version(input: &str) -> Option<ParsedVersion> {
    let selector = parse_selector(input)?;
    match selector {
        VersionSelector::Major(major) => Some(ParsedVersion {
            major,
            minor: 0,
            patch: 0,
        }),
        VersionSelector::MajorMinor(major, minor) => Some(ParsedVersion {
            major,
            minor,
            patch: 0,
        }),
        VersionSelector::Exact(version) => Some(version),
    }
}

fn major_string(input: &str) -> Option<String> {
    parse_version(input).map(|version| version.major.to_string())
}

enum VersionSelector {
    Major(u64),
    MajorMinor(u64, u64),
    Exact(ParsedVersion),
}

impl VersionSelector {
    fn matches(&self, version: &str) -> bool {
        let Some(parsed) = parse_version(version) else {
            return false;
        };
        match self {
            VersionSelector::Major(major) => parsed.major == *major,
            VersionSelector::MajorMinor(major, minor) => {
                parsed.major == *major && parsed.minor == *minor
            }
            VersionSelector::Exact(expected) => parsed == *expected,
        }
    }
}

impl Ord for ParsedVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl PartialOrd for ParsedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_outdated_payload, build_read_configuration_payload, feature_id_without_version,
        lockfile_path, run_upgrade_lockfile, should_use_native_read_configuration,
    };
    use crate::commands::common::resolve_read_configuration_path;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let unique_id = NEXT_TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "devcontainer-config-command-test-{}-{suffix}-{unique_id}",
            std::process::id()
        ))
    }

    #[test]
    fn resolves_modern_config_path_from_workspace_folder() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        let config = config_dir.join("devcontainer.json");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
        let result = resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            result.1,
            fs::canonicalize(config).expect("failed to canonicalize")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fails_when_explicit_config_file_is_missing() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create root");
        let missing_config = root.join("missing.json");
        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--config".to_string(),
            missing_config.display().to_string(),
        ];

        let result = resolve_read_configuration_path(&args);

        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fails_when_workspace_folder_option_is_missing_a_value() {
        let result = resolve_read_configuration_path(&["--workspace-folder".to_string()]);

        assert_eq!(
            result.expect_err("expected missing option value"),
            "Missing value for option: --workspace-folder"
        );
    }

    #[test]
    fn resolves_relative_config_against_workspace_folder() {
        let root = unique_temp_dir();
        let config = root.join("relative.devcontainer.json");
        fs::create_dir_all(&root).expect("failed to create root");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--config".to_string(),
            "relative.devcontainer.json".to_string(),
        ];
        let result = resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            result.1,
            fs::canonicalize(config).expect("failed to canonicalize")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn infers_workspace_root_for_nested_devcontainer_configs() {
        let root = unique_temp_dir();
        let nested_config_dir = root.join(".devcontainer").join("python");
        let config = nested_config_dir.join("devcontainer.json");
        fs::create_dir_all(&root).expect("failed to create root");
        fs::create_dir_all(&nested_config_dir).expect("failed to create nested config directory");
        fs::write(&config, "{}").expect("failed to write config");

        let args = vec!["--config".to_string(), config.display().to_string()];
        let (workspace_folder, config_file) =
            resolve_read_configuration_path(&args).expect("expected config resolution");

        assert_eq!(
            workspace_folder,
            fs::canonicalize(&root).expect("failed to canonicalize workspace")
        );
        assert_eq!(
            config_file,
            fs::canonicalize(config).expect("failed to canonicalize config")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_configuration_with_additional_flags_is_supported_natively() {
        assert!(should_use_native_read_configuration(&[
            "--workspace-folder".to_string(),
            "/workspace".to_string(),
            "--include-merged-configuration".to_string(),
        ]));
    }

    #[test]
    fn read_configuration_accepts_docker_compose_path_flag() {
        assert!(should_use_native_read_configuration(&[
            "--workspace-folder".to_string(),
            "/workspace".to_string(),
            "--docker-compose-path".to_string(),
            "trigger-compose-v2".to_string(),
        ]));
    }

    #[test]
    fn read_configuration_payload_includes_optional_sections() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": { \"ghcr.io/devcontainers/features/git:1\": {} }\n}\n",
        )
        .expect("failed to write config");

        let args = vec![
            "--workspace-folder".to_string(),
            root.display().to_string(),
            "--include-merged-configuration".to_string(),
            "--include-features-configuration".to_string(),
        ];
        let payload = build_read_configuration_payload(&args).expect("payload");

        assert_eq!(payload["configuration"]["image"], "debian:bookworm");
        assert_eq!(payload["mergedConfiguration"]["image"], "debian:bookworm");
        assert!(payload["featuresConfiguration"]["features"]
            .as_object()
            .expect("features object")
            .contains_key("ghcr.io/devcontainers/features/git:1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn outdated_payload_reports_remote_feature_versions() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create root");
        fs::write(
            root.join(".devcontainer.json"),
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/git:1.0\": \"latest\",\n    \"./local-feature\": {}\n  }\n}\n",
        )
        .expect("failed to write config");
        fs::write(
            root.join(".devcontainer-lock.json"),
            "{\n  \"features\": {\n    \"ghcr.io/devcontainers/features/git:1.0\": {\n      \"version\": \"1.0.4\",\n      \"resolved\": \"ghcr.io/devcontainers/features/git@sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6\",\n      \"integrity\": \"sha256:0bb490abcc0a3fb23937d29e2c18a225b51c5584edc0d9eb4131569a980f60b6\"\n    }\n  }\n}\n",
        )
        .expect("failed to write lockfile");

        let args = vec!["--workspace-folder".to_string(), root.display().to_string()];
        let payload = build_outdated_payload(&args).expect("payload");

        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["current"],
            "1.0.4"
        );
        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["wanted"],
            "1.0.5"
        );
        assert_eq!(
            payload["features"]["ghcr.io/devcontainers/features/git:1.0"]["latest"],
            "1.2.0"
        );
        assert!(payload["features"]["./local-feature"].is_null());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upgrade_lockfile_uses_root_relative_lockfile_for_dotfile_configs() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create root");
        fs::write(
            root.join(".devcontainer.json"),
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"ghcr.io/devcontainers/features/github-cli\": \"latest\"\n  }\n}\n",
        )
        .expect("failed to write config");

        let lockfile =
            run_upgrade_lockfile(&["--workspace-folder".to_string(), root.display().to_string()])
                .expect("lockfile payload");

        let lockfile_path = root.join(".devcontainer-lock.json");
        assert!(lockfile_path.is_file());
        assert_eq!(
            lockfile.features["ghcr.io/devcontainers/features/github-cli"].version,
            "1.0.9"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn feature_id_without_version_handles_tags_and_digests() {
        assert_eq!(
            feature_id_without_version("ghcr.io/devcontainers/features/git:1.0"),
            "ghcr.io/devcontainers/features/git"
        );
        assert_eq!(
            feature_id_without_version(
                "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c"
            ),
            "ghcr.io/devcontainers/features/git-lfs"
        );
    }

    #[test]
    fn lockfile_path_matches_upstream_dotfile_rule() {
        assert_eq!(
            lockfile_path(Path::new("/tmp/workspace/.devcontainer.json")),
            PathBuf::from("/tmp/workspace/.devcontainer-lock.json")
        );
        assert_eq!(
            lockfile_path(Path::new("/tmp/workspace/.devcontainer/devcontainer.json")),
            PathBuf::from("/tmp/workspace/.devcontainer/devcontainer-lock.json")
        );
    }
}
