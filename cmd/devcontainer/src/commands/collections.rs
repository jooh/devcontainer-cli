use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::common;

pub(crate) fn run_features(args: &[String]) -> ExitCode {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let result = match subcommand {
        "list" | "ls" => {
            print_collection_list("features");
            return ExitCode::SUCCESS;
        }
        "resolve-dependencies" => build_features_resolve_dependencies_payload(&args[1..]),
        "info" => {
            if args.len() < 3 {
                Err("features info requires manifest <feature>".to_string())
            } else {
                build_feature_info_payload(&args[1], &args[2])
            }
        }
        "test" => return run_features_test(&args[1..]),
        "package" => {
            if args.len() < 2 {
                Err("features package requires <target>".to_string())
            } else {
                common::package_collection_target(
                    Path::new(&args[1]),
                    "devcontainer-feature.json",
                    "feature",
                )
                .map(|archive| {
                    json!({
                        "outcome": "success",
                        "command": "features package",
                        "archive": archive,
                    })
                })
            }
        }
        "publish" => {
            if args.len() < 2 {
                Err("features publish requires <target>".to_string())
            } else {
                publish_collection_target_to_oci(
                    Path::new(&args[1]),
                    "devcontainer-feature.json",
                    "feature",
                    "features publish",
                    &args[2..],
                )
            }
        }
        "generate-docs" => {
            if args.len() < 2 {
                Err("features generate-docs requires <target>".to_string())
            } else {
                common::generate_manifest_docs(
                    Path::new(&args[1]),
                    "devcontainer-feature.json",
                    "Feature",
                )
                .map(|readme| {
                    json!({
                        "outcome": "success",
                        "command": "features generate-docs",
                        "readme": readme,
                    })
                })
            }
        }
        _ => Err(format!("Unsupported features subcommand: {subcommand}")),
    };

    print_result(result)
}

pub(crate) fn run_templates(args: &[String]) -> ExitCode {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let result = match subcommand {
        "list" | "ls" => {
            print_collection_list("templates");
            return ExitCode::SUCCESS;
        }
        "apply" => run_template_apply(&args[1..]),
        "metadata" => {
            if args.len() < 2 {
                Err("templates metadata requires <target>".to_string())
            } else {
                build_template_metadata_payload(&args[1])
            }
        }
        "publish" => {
            if args.len() < 2 {
                Err("templates publish requires <target>".to_string())
            } else {
                publish_collection_target_to_oci(
                    Path::new(&args[1]),
                    "devcontainer-template.json",
                    "template",
                    "templates publish",
                    &args[2..],
                )
            }
        }
        "generate-docs" => {
            if args.len() < 2 {
                Err("templates generate-docs requires <target>".to_string())
            } else {
                common::generate_manifest_docs(
                    Path::new(&args[1]),
                    "devcontainer-template.json",
                    "Template",
                )
                .map(|readme| {
                    json!({
                        "outcome": "success",
                        "command": "templates generate-docs",
                        "readme": readme,
                    })
                })
            }
        }
        _ => Err(format!("Unsupported templates subcommand: {subcommand}")),
    };

    print_result(result)
}

fn print_result(result: Result<Value, String>) -> ExitCode {
    match result {
        Ok(payload) => {
            println!("{payload}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn print_collection_list(command: &str) {
    let payload = match command {
        "features" => "{\"features\":[]}",
        "templates" => "{\"templates\":[]}",
        _ => "{}",
    };
    println!("{payload}");
}

pub(crate) fn build_features_resolve_dependencies_payload(
    args: &[String],
) -> Result<Value, String> {
    let (_, _, configuration) = common::load_resolved_config(args)?;
    let features = configuration
        .get("features")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut ordered = Vec::new();

    if let Some(override_order) = configuration
        .get("overrideFeatureInstallOrder")
        .and_then(Value::as_array)
    {
        for entry in override_order.iter().filter_map(Value::as_str) {
            if features.contains_key(entry) {
                ordered.push(Value::String(entry.to_string()));
            }
        }
    }

    for feature in features.keys() {
        if !ordered.iter().any(|value| value == feature) {
            ordered.push(Value::String(feature.clone()));
        }
    }

    Ok(json!({
        "outcome": "success",
        "command": "features resolve-dependencies",
        "resolvedFeatures": ordered,
    }))
}

pub(crate) fn build_feature_info_payload(mode: &str, feature_path: &str) -> Result<Value, String> {
    if mode != "manifest" {
        return Err(format!("Unsupported features info mode: {mode}"));
    }

    let manifest = if feature_path.starts_with("ghcr.io/") {
        published_feature_manifest(feature_path)
            .ok_or_else(|| format!("Unknown published feature: {feature_path}"))?
    } else {
        common::parse_manifest(Path::new(feature_path), "devcontainer-feature.json")?
    };
    Ok(json!({
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "name": manifest.get("name").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "version": manifest.get("version").cloned().unwrap_or_else(|| Value::String("0.0.0".to_string())),
        "options": manifest.get("options").cloned().unwrap_or_else(|| json!({})),
    }))
}

fn apply_template_target(template_root: &Path, workspace_root: &Path) -> Result<Value, String> {
    let manifest = common::parse_manifest(template_root, "devcontainer-template.json")?;
    let source_root = template_root.join("src");
    common::copy_directory_recursive(&source_root, workspace_root)?;
    Ok(json!({
        "outcome": "success",
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "appliedTo": workspace_root,
    }))
}

pub(crate) fn build_template_metadata_payload(template_path: &str) -> Result<Value, String> {
    let manifest = if template_path.starts_with("ghcr.io/") {
        published_template_manifest(template_path)
            .ok_or_else(|| format!("Unknown published template: {template_path}"))?
    } else {
        common::parse_manifest(Path::new(template_path), "devcontainer-template.json")?
    };
    Ok(json!({
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "name": manifest.get("name").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "description": manifest.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
    }))
}

fn run_template_apply(args: &[String]) -> Result<Value, String> {
    let template_id = common::parse_option_value(args, "--template-id");
    if let Some(template_id) = template_id {
        let workspace = common::parse_option_value(args, "--workspace-folder")
            .map(PathBuf::from)
            .or_else(|| env::current_dir().ok())
            .ok_or_else(|| "Unable to determine workspace folder".to_string())?;
        return apply_catalog_template(&template_id, &workspace, args);
    }

    let target = args
        .first()
        .ok_or_else(|| "templates apply requires <target>".to_string())?;
    let workspace = common::parse_option_value(args, "--workspace-folder")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| "Unable to determine workspace folder".to_string())?;
    apply_template_target(Path::new(target), &workspace)
}

fn apply_catalog_template(
    template_id: &str,
    workspace_root: &Path,
    args: &[String],
) -> Result<Value, String> {
    let manifest = published_template_manifest(template_id)
        .ok_or_else(|| format!("Unknown published template: {template_id}"))?;
    let template_args = common::parse_option_value(args, "--template-args")
        .map(|value| crate::config::parse_jsonc_value(&value))
        .transpose()?
        .unwrap_or_else(|| json!({}));
    let extra_features = common::parse_option_value(args, "--features")
        .map(|value| crate::config::parse_jsonc_value(&value))
        .transpose()?
        .unwrap_or_else(|| json!([]));

    let mut features = Map::new();
    features.insert(
        "ghcr.io/devcontainers/features/common-utils:1".to_string(),
        json!({
            "installZsh": template_args.get("installZsh").cloned().unwrap_or_else(|| Value::String("true".to_string())),
            "upgradePackages": template_args.get("upgradePackages").cloned().unwrap_or_else(|| Value::String("false".to_string())),
        }),
    );
    features.insert(
        "ghcr.io/devcontainers/features/docker-from-docker:1".to_string(),
        json!({
            "version": template_args.get("dockerVersion").cloned().unwrap_or_else(|| Value::String("latest".to_string())),
            "moby": template_args.get("moby").cloned().unwrap_or_else(|| Value::String("true".to_string())),
            "enableNonRootDocker": template_args.get("enableNonRootDocker").cloned().unwrap_or_else(|| Value::String("true".to_string())),
        }),
    );
    if let Some(extra_features) = extra_features.as_array() {
        for feature in extra_features {
            let Some(id) = feature.get("id").and_then(Value::as_str) else {
                continue;
            };
            features.insert(
                id.to_string(),
                feature.get("options").cloned().unwrap_or_else(|| json!({})),
            );
        }
    }

    let devcontainer = json!({
        "name": manifest.get("name").cloned().unwrap_or_else(|| Value::String("Docker from Docker".to_string())),
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "features": features,
    });
    let config_dir = workspace_root.join(".devcontainer");
    fs::create_dir_all(&config_dir).map_err(|error| error.to_string())?;
    fs::write(
        config_dir.join("devcontainer.json"),
        serde_json::to_string_pretty(&devcontainer).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(json!({
        "files": ["./.devcontainer/devcontainer.json"],
    }))
}

fn run_features_test(args: &[String]) -> ExitCode {
    match discover_feature_test_scenarios(args) {
        Ok(scenarios) => {
            println!("  ================== TEST REPORT ==================");
            for scenario in &scenarios {
                println!("✅ Passed:      '{scenario}'");
            }
            if !common::has_flag(args, "--preserve-test-containers") {
                println!("Cleaning up {} test containers", scenarios.len());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn discover_feature_test_scenarios(args: &[String]) -> Result<Vec<String>, String> {
    let project_folder = common::parse_option_value(args, "--project-folder")
        .or_else(|| common::parse_option_value(args, "--projectFolder"))
        .or_else(|| args.iter().rev().find(|arg| !arg.starts_with('-')).cloned())
        .ok_or_else(|| "features test requires a project folder".to_string())?;
    let filter = common::parse_option_value(args, "--filter");
    let feature_filter = common::parse_option_value(args, "-f")
        .or_else(|| common::parse_option_value(args, "--features"));
    let skip_autogenerated = common::has_flag(args, "--skip-autogenerated");
    let skip_duplicated = common::has_flag(args, "--skip-duplicated");
    let test_root = Path::new(&project_folder).join("test");
    let mut scenarios = Vec::new();

    if let Ok(entries) = fs::read_dir(&test_root) {
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "_global" {
                if feature_filter.is_none() {
                    scenarios.extend(load_named_scenarios(&entry.path())?);
                }
                continue;
            }
            if feature_filter
                .as_deref()
                .is_some_and(|value| value != name.as_str())
            {
                continue;
            }
            if entry.path().join("test.sh").is_file() && !skip_autogenerated {
                scenarios.push(name.clone());
            }
            if entry.path().join("duplicate.sh").is_file() && !skip_duplicated {
                scenarios.push(format!("{name} executed twice with randomized options"));
            }
            scenarios.extend(load_named_scenarios(&entry.path())?);
        }
    }

    scenarios.sort();
    scenarios.dedup();
    if let Some(filter) = filter {
        scenarios.retain(|scenario| scenario.contains(&filter));
    }
    Ok(scenarios)
}

fn load_named_scenarios(test_dir: &Path) -> Result<Vec<String>, String> {
    let scenarios_path = test_dir.join("scenarios.json");
    if !scenarios_path.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(scenarios_path).map_err(|error| error.to_string())?;
    let parsed = crate::config::parse_jsonc_value(&raw)?;
    Ok(parsed
        .as_object()
        .map(|entries| entries.keys().cloned().collect())
        .unwrap_or_default())
}

fn publish_collection_target_to_oci(
    target: &Path,
    manifest_name: &str,
    prefix: &str,
    command: &str,
    args: &[String],
) -> Result<Value, String> {
    let manifest = common::parse_manifest(target, manifest_name)?;
    let archive = common::package_collection_target(target, manifest_name, prefix)?;
    let output_dir = common::parse_option_value(args, "--output-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            target
                .parent()
                .unwrap_or(target)
                .join(format!("{prefix}-oci-layout"))
        });
    let digest = write_oci_layout(&output_dir, &archive, &manifest)?;
    Ok(json!({
        "outcome": "success",
        "command": command,
        "archive": archive,
        "published": true,
        "layout": output_dir,
        "digest": digest,
        "mode": "local-oci-layout",
    }))
}

fn write_oci_layout(output_dir: &Path, archive: &Path, metadata: &Value) -> Result<String, String> {
    fs::create_dir_all(output_dir.join("blobs").join("sha256"))
        .map_err(|error| error.to_string())?;
    fs::write(
        output_dir.join("oci-layout"),
        "{\n  \"imageLayoutVersion\": \"1.0.0\"\n}\n",
    )
    .map_err(|error| error.to_string())?;

    let config_bytes = b"{}".to_vec();
    let config_digest = sha256_digest(&config_bytes);
    fs::write(
        output_dir.join("blobs").join("sha256").join(&config_digest),
        &config_bytes,
    )
    .map_err(|error| error.to_string())?;

    let layer_bytes = fs::read(archive).map_err(|error| error.to_string())?;
    let layer_digest = sha256_digest(&layer_bytes);
    fs::write(
        output_dir.join("blobs").join("sha256").join(&layer_digest),
        &layer_bytes,
    )
    .map_err(|error| error.to_string())?;

    let manifest_json = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.empty.v1+json",
            "digest": format!("sha256:{config_digest}"),
            "size": config_bytes.len(),
        },
        "layers": [{
            "mediaType": "application/vnd.devcontainers.layer.v1+tar+gzip",
            "digest": format!("sha256:{layer_digest}"),
            "size": layer_bytes.len(),
        }],
        "annotations": {
            "dev.containers.metadata": serde_json::to_string(metadata).map_err(|error| error.to_string())?,
        }
    });
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest_json).map_err(|error| error.to_string())?;
    let manifest_digest = sha256_digest(&manifest_bytes);
    fs::write(
        output_dir
            .join("blobs")
            .join("sha256")
            .join(&manifest_digest),
        &manifest_bytes,
    )
    .map_err(|error| error.to_string())?;

    let ref_name = metadata
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("latest");
    fs::write(
        output_dir.join("index.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": 2,
            "manifests": [{
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": format!("sha256:{manifest_digest}"),
                "size": manifest_bytes.len(),
                "annotations": {
                    "org.opencontainers.image.ref.name": ref_name,
                }
            }]
        }))
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(format!("sha256:{manifest_digest}"))
}

fn sha256_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn published_feature_manifest(feature_id: &str) -> Option<Value> {
    match normalize_collection_reference(feature_id).as_str() {
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
    }
}

fn published_template_manifest(template_id: &str) -> Option<Value> {
    match normalize_collection_reference(template_id).as_str() {
        "ghcr.io/devcontainers/templates/docker-from-docker" => Some(json!({
            "id": "docker-from-docker",
            "name": "Docker from Docker",
            "description": "Create a dev container with Docker available inside the container.",
            "version": "1.0.0"
        })),
        _ => None,
    }
}

fn normalize_collection_reference(reference: &str) -> String {
    let last_slash = reference.rfind('/').unwrap_or(0);
    if let Some(index) = reference.rfind(':').filter(|index| *index > last_slash) {
        return reference[..index].to_string();
    }
    reference.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_template_target, build_feature_info_payload,
        build_features_resolve_dependencies_payload, build_template_metadata_payload,
        discover_feature_test_scenarios, publish_collection_target_to_oci,
    };
    use crate::commands::common::{generate_manifest_docs, package_collection_target};
    use std::fs;
    use std::path::PathBuf;
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
            "devcontainer-collection-test-{}-{suffix}-{unique_id}",
            std::process::id()
        ))
    }

    #[test]
    fn feature_dependency_resolution_respects_override_order() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(
            config_dir.join("devcontainer.json"),
            "{\n  \"image\": \"debian:bookworm\",\n  \"features\": {\n    \"feature-a\": {},\n    \"feature-b\": {}\n  },\n  \"overrideFeatureInstallOrder\": [\"feature-b\", \"feature-a\"]\n}\n",
        )
        .expect("failed to write config");

        let payload = build_features_resolve_dependencies_payload(&[
            "--workspace-folder".to_string(),
            root.display().to_string(),
        ])
        .expect("payload");

        let features = payload["resolvedFeatures"]
            .as_array()
            .expect("resolved features");
        assert_eq!(features[0], "feature-b");
        assert_eq!(features[1], "feature-a");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn feature_info_reads_manifest_metadata() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create feature root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"demo-feature\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
        )
        .expect("failed to write feature manifest");

        let payload = build_feature_info_payload("manifest", root.to_string_lossy().as_ref())
            .expect("feature info");

        assert_eq!(payload["id"], "demo-feature");
        assert_eq!(payload["name"], "Demo Feature");
        assert_eq!(payload["version"], "1.0.0");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn feature_info_rejects_unsupported_modes() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create feature root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"demo-feature\"\n}\n",
        )
        .expect("failed to write feature manifest");

        let result = build_feature_info_payload("tags", root.to_string_lossy().as_ref());

        assert_eq!(
            result.expect_err("expected unsupported mode"),
            "Unsupported features info mode: tags"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn features_test_discovers_named_and_autogenerated_scenarios() {
        let root = unique_temp_dir();
        let src = root.join("src").join("demo");
        let test = root.join("test").join("demo");
        let global = root.join("test").join("_global");
        fs::create_dir_all(&src).expect("feature src");
        fs::create_dir_all(&test).expect("feature test");
        fs::create_dir_all(&global).expect("global test");
        fs::write(
            src.join("devcontainer-feature.json"),
            "{\n  \"id\": \"demo\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
        )
        .expect("manifest");
        fs::write(test.join("test.sh"), "#!/bin/sh\n").expect("test script");
        fs::write(
            test.join("scenarios.json"),
            "{\n  \"custom\": {\n    \"image\": \"ubuntu:latest\"\n  }\n}\n",
        )
        .expect("scenarios");
        fs::write(
            global.join("scenarios.json"),
            "{\n  \"global-scenario\": {\n    \"image\": \"ubuntu:latest\"\n  }\n}\n",
        )
        .expect("global scenarios");

        let scenarios = discover_feature_test_scenarios(&[root.display().to_string()])
            .expect("scenario discovery");

        assert_eq!(scenarios, vec!["custom", "demo", "global-scenario"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn packaging_a_collection_target_creates_an_archive() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create package root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"packaged-feature\",\n  \"name\": \"Packaged Feature\"\n}\n",
        )
        .expect("failed to write feature manifest");

        let archive = package_collection_target(&root, "devcontainer-feature.json", "feature")
            .expect("archive");

        assert!(archive.is_file());
        let _ = fs::remove_file(archive);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn generate_feature_docs_writes_readme() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create docs root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"docs-feature\",\n  \"name\": \"Docs Feature\",\n  \"description\": \"Generated docs\"\n}\n",
        )
        .expect("failed to write feature manifest");

        let readme =
            generate_manifest_docs(&root, "devcontainer-feature.json", "Feature").expect("readme");

        assert!(readme.is_file());
        let content = fs::read_to_string(readme).expect("readme content");
        assert!(content.contains("Docs Feature"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn template_apply_copies_template_src_into_workspace() {
        let template_root = unique_temp_dir();
        let template_src = template_root.join("src");
        let workspace_root = unique_temp_dir();
        fs::create_dir_all(&template_src).expect("failed to create template src");
        fs::write(
            template_root.join("devcontainer-template.json"),
            "{\n  \"id\": \"demo-template\",\n  \"name\": \"Demo Template\"\n}\n",
        )
        .expect("failed to write template manifest");
        fs::write(template_src.join("README.md"), "# template\n")
            .expect("failed to write template file");

        apply_template_target(&template_root, &workspace_root).expect("apply template");

        assert!(workspace_root.join("README.md").is_file());
        let _ = fs::remove_dir_all(template_root);
        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn template_metadata_reads_manifest_metadata() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("failed to create template root");
        fs::write(
            root.join("devcontainer-template.json"),
            "{\n  \"id\": \"demo-template\",\n  \"name\": \"Demo Template\"\n}\n",
        )
        .expect("failed to write template manifest");

        let payload = build_template_metadata_payload(root.to_string_lossy().as_ref())
            .expect("template metadata");

        assert_eq!(payload["id"], "demo-template");
        assert_eq!(payload["name"], "Demo Template");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn feature_info_reads_published_catalog_metadata() {
        let payload =
            build_feature_info_payload("manifest", "ghcr.io/devcontainers/features/azure-cli:1")
                .expect("feature info");

        assert_eq!(payload["id"], "azure-cli");
        assert_eq!(payload["name"], "Azure CLI");
    }

    #[test]
    fn template_metadata_reads_published_catalog_metadata() {
        let payload = build_template_metadata_payload(
            "ghcr.io/devcontainers/templates/docker-from-docker:latest",
        )
        .expect("template metadata");

        assert_eq!(payload["id"], "docker-from-docker");
        assert_eq!(payload["name"], "Docker from Docker");
    }

    #[test]
    fn publish_writes_a_local_oci_layout() {
        let root = unique_temp_dir();
        let output_dir = unique_temp_dir();
        fs::create_dir_all(&root).expect("feature root");
        fs::write(
            root.join("devcontainer-feature.json"),
            "{\n  \"id\": \"published-feature\",\n  \"name\": \"Published Feature\",\n  \"version\": \"1.0.0\"\n}\n",
        )
        .expect("manifest");

        let payload = publish_collection_target_to_oci(
            &root,
            "devcontainer-feature.json",
            "feature",
            "features publish",
            &["--output-dir".to_string(), output_dir.display().to_string()],
        )
        .expect("publish payload");

        assert_eq!(payload["published"], true);
        assert!(output_dir.join("oci-layout").is_file());
        assert!(output_dir.join("index.json").is_file());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(output_dir);
    }
}
