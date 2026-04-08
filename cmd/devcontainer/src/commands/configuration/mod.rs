mod catalog;
mod inspect;
mod load;
mod merge;
mod read;
mod upgrade;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Lockfile {
    features: BTreeMap<String, LockfileEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct LockfileEntry {
    version: String,
    resolved: String,
    integrity: String,
    #[serde(rename = "dependsOn", skip_serializing_if = "Option::is_none")]
    depends_on: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CatalogEntry {
    version: String,
    resolved: String,
    integrity: String,
    depends_on: Option<Vec<String>>,
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

struct InspectedContainer {
    metadata_entries: Vec<Value>,
    container_env: HashMap<String, String>,
}

pub(crate) fn build_read_configuration_payload(args: &[String]) -> Result<Value, String> {
    read::build_read_configuration_payload(args)
}

pub(crate) fn should_use_native_read_configuration(args: &[String]) -> bool {
    read::should_use_native_read_configuration(args)
}

pub(crate) fn run_outdated(args: &[String]) -> ExitCode {
    upgrade::run_outdated(args)
}

pub(crate) fn run_upgrade(args: &[String]) -> ExitCode {
    upgrade::run_upgrade(args)
}

#[cfg(test)]
mod tests;
