use std::env;
use std::path::Path;
use std::process::ExitCode;

use serde_json::{json, Value};

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
        "test" => Err("Native features test is not implemented".to_string()),
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
                common::package_collection_target(
                    Path::new(&args[1]),
                    "devcontainer-feature.json",
                    "feature",
                )
                .map(|archive| {
                    json!({
                        "outcome": "success",
                        "command": "features publish",
                        "archive": archive,
                        "published": false,
                        "mode": "local-package-only",
                    })
                })
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
        "apply" => {
            if args.len() < 2 {
                Err("templates apply requires <target>".to_string())
            } else {
                match env::current_dir().map_err(|error| error.to_string()) {
                    Ok(workspace) => apply_template_target(Path::new(&args[1]), &workspace),
                    Err(error) => Err(error),
                }
            }
        }
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
                common::package_collection_target(
                    Path::new(&args[1]),
                    "devcontainer-template.json",
                    "template",
                )
                .map(|archive| {
                    json!({
                        "outcome": "success",
                        "command": "templates publish",
                        "archive": archive,
                        "published": false,
                        "mode": "local-package-only",
                    })
                })
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

    let manifest = common::parse_manifest(Path::new(feature_path), "devcontainer-feature.json")?;
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
    let manifest = common::parse_manifest(Path::new(template_path), "devcontainer-template.json")?;
    Ok(json!({
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "name": manifest.get("name").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "description": manifest.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_template_target, build_feature_info_payload,
        build_features_resolve_dependencies_payload, build_template_metadata_payload, run_features,
    };
    use crate::commands::common::{generate_manifest_docs, package_collection_target};
    use std::fs;
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("devcontainer-collection-test-{suffix}"))
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
    fn features_test_returns_failure_until_native_implementation_exists() {
        let status = run_features(&["test".to_string()]);

        assert_eq!(status, ExitCode::from(1));
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
}
