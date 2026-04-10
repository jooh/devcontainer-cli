//! Shared data structures for feature resolution, metadata, and installation materialization.

use std::path::PathBuf;

use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) enum FeatureInstallationSource {
    Local(PathBuf),
    Published(String),
}

#[derive(Clone, Debug)]
pub(crate) struct FeatureInstallation {
    pub(crate) source: FeatureInstallationSource,
    pub(crate) env: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedFeatureSupport {
    pub(crate) features_configuration: Value,
    pub(crate) metadata_entries: Vec<Value>,
    pub(crate) installations: Vec<FeatureInstallation>,
    pub(crate) ordered_feature_ids: Vec<String>,
}

#[derive(Clone)]
pub(super) struct FeatureSpec {
    pub(super) manifest: Value,
    pub(super) options: Value,
    pub(super) source_information: Value,
    pub(super) metadata_entry: Value,
    pub(super) installation: FeatureInstallation,
    pub(super) depends_on: Vec<String>,
}
