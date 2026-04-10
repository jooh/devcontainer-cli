//! Feature installation naming and on-disk materialization helpers.

use std::fs;
use std::path::Path;

use crate::commands::collections::registry::{
    collection_slug, published_feature_install_script, published_feature_manifest,
};
use crate::commands::common;

use super::types::{FeatureInstallation, FeatureInstallationSource};

pub(crate) fn materialize_feature_installation(
    installation: &FeatureInstallation,
    destination: &Path,
) -> Result<(), String> {
    match &installation.source {
        FeatureInstallationSource::Local(path) => {
            common::copy_directory_recursive(path, destination)?;
            ensure_feature_install_script(destination)
        }
        FeatureInstallationSource::Published(feature_id) => {
            let manifest = published_feature_manifest(feature_id)
                .ok_or_else(|| format!("Unknown published feature: {feature_id}"))?;
            fs::create_dir_all(destination).map_err(|error| error.to_string())?;
            fs::write(
                destination.join("devcontainer-feature.json"),
                serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            fs::write(
                destination.join("install.sh"),
                published_feature_install_script(feature_id),
            )
            .map_err(|error| error.to_string())?;
            ensure_feature_install_script(destination)
        }
    }
}

pub(crate) fn feature_installation_name(installation: &FeatureInstallation) -> String {
    match &installation.source {
        FeatureInstallationSource::Local(path) => path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("feature")
            .to_string(),
        FeatureInstallationSource::Published(feature_id) => {
            collection_slug(feature_id).unwrap_or_else(|| "published-feature".to_string())
        }
    }
}

fn ensure_feature_install_script(destination: &Path) -> Result<(), String> {
    let install_path = destination.join("install.sh");
    if install_path.is_file() {
        return Ok(());
    }
    fs::write(&install_path, "#!/bin/sh\nset -eu\n").map_err(|error| error.to_string())
}
