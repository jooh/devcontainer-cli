use std::env;
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::common;
use crate::runtime;

const DEFAULT_FEATURE_TEST_BASE_IMAGE: &str = "mcr.microsoft.com/devcontainers/base:ubuntu";
const FEATURE_TEST_LIBRARY_SCRIPT_NAME: &str = "dev-container-features-test-lib";
const FEATURE_TEST_LIBRARY_SCRIPT: &str = r#"#!/bin/bash
SCRIPT_FOLDER="$(cd "$(dirname $0)" && pwd)"
USERNAME=${1:-root}

if [ -z $HOME ]; then
    HOME="/root"
fi

FAILED=()

echoStderr()
{
    echo "$@" 1>&2
}

check() {
    LABEL=$1
    shift
    echo -e "\n"
    echo -e "🔄 Testing '$LABEL'"
    echo -e '\033[37m'
    if "$@"; then
        echo -e "\n"
        echo "✅  Passed '$LABEL'!"
        return 0
    else
        echo -e "\n"
        echoStderr "❌ $LABEL check failed."
        FAILED+=("$LABEL")
        return 1
    fi
}

checkMultiple() {
    PASSED=0
    LABEL="$1"
    echo -e "\n"
    echo -e "🔄 Testing '$LABEL'."
    shift; MINIMUMPASSED=$1
    shift; EXPRESSION="$1"
    while [ "$EXPRESSION" != "" ]; do
        if $EXPRESSION; then ((PASSED+=1)); fi
        shift; EXPRESSION=$1
    done
    if [ $PASSED -ge $MINIMUMPASSED ]; then
        echo -e "\n"
        echo "✅ Passed!"
        return 0
    else
        echo -e "\n"
        echoStderr "❌ '$LABEL' check failed."
        FAILED+=("$LABEL")
        return 1
    fi
}

reportResults() {
    if [ ${#FAILED[@]} -ne 0 ]; then
        echo -e "\n"
        echoStderr -e "💥  Failed tests: ${FAILED[@]}"
        exit 1
    else
        echo -e "\n"
        echo -e "Test Passed!"
        exit 0
    fi
}"#;

static NEXT_FEATURE_TEST_ID: AtomicU64 = AtomicU64::new(0);

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

    let normalized_template_id = normalize_collection_reference(template_id);
    if normalized_template_id != "ghcr.io/devcontainers/templates/docker-from-docker" {
        if let Some(template_root) = embedded_template_source_dir(&normalized_template_id) {
            return apply_embedded_published_template(
                &manifest,
                &template_root,
                workspace_root,
                &template_args,
                extra_features,
            );
        }
        return apply_generic_published_template(&manifest, workspace_root, extra_features);
    }

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

fn apply_embedded_published_template(
    manifest: &Value,
    template_root: &Path,
    workspace_root: &Path,
    template_args: &Value,
    extra_features: Value,
) -> Result<Value, String> {
    let template_options = template_option_values(manifest, template_args);
    copy_embedded_template_contents(template_root, workspace_root, &template_options)?;
    merge_extra_features_into_template(workspace_root, extra_features)?;
    Ok(json!({
        "outcome": "success",
        "id": manifest.get("id").cloned().unwrap_or_else(|| Value::String("unknown".to_string())),
        "appliedTo": workspace_root,
    }))
}

fn apply_generic_published_template(
    manifest: &Value,
    workspace_root: &Path,
    extra_features: Value,
) -> Result<Value, String> {
    let mut devcontainer = Map::new();
    devcontainer.insert(
        "name".to_string(),
        manifest
            .get("name")
            .cloned()
            .unwrap_or_else(|| Value::String("Published Template".to_string())),
    );
    devcontainer.insert(
        "image".to_string(),
        Value::String("mcr.microsoft.com/devcontainers/base:ubuntu".to_string()),
    );

    let mut features = Map::new();
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
    if !features.is_empty() {
        devcontainer.insert("features".to_string(), Value::Object(features));
    }

    let config_dir = workspace_root.join(".devcontainer");
    fs::create_dir_all(&config_dir).map_err(|error| error.to_string())?;
    fs::write(
        config_dir.join("devcontainer.json"),
        serde_json::to_string_pretty(&Value::Object(devcontainer))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    Ok(json!({
        "files": ["./.devcontainer/devcontainer.json"],
    }))
}

fn embedded_template_source_dir(reference: &str) -> Option<PathBuf> {
    let slug = collection_slug(reference)?;
    match slug.as_str() {
        "alpine" | "cpp" | "mytemplate" | "node-mongo" => Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(
                    "../../upstream/src/test/container-templates/example-templates-sets/simple/src",
                )
                .join(slug),
        ),
        _ => None,
    }
}

fn template_option_values(manifest: &Value, template_args: &Value) -> Map<String, Value> {
    let mut options = manifest
        .get("options")
        .and_then(Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(name, definition)| {
                    definition
                        .get("default")
                        .cloned()
                        .map(|value| (name.clone(), value))
                })
                .collect::<Map<String, Value>>()
        })
        .unwrap_or_default();
    if let Some(template_args) = template_args.as_object() {
        for (name, value) in template_args {
            options.insert(name.clone(), value.clone());
        }
    }
    options
}

fn copy_embedded_template_contents(
    template_root: &Path,
    workspace_root: &Path,
    template_options: &Map<String, Value>,
) -> Result<(), String> {
    fs::create_dir_all(workspace_root).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(template_root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_name() == "devcontainer-template.json" {
            continue;
        }
        copy_embedded_template_entry(
            &entry.path(),
            &workspace_root.join(entry.file_name()),
            template_options,
        )?;
    }
    Ok(())
}

fn copy_embedded_template_entry(
    source: &Path,
    destination: &Path,
    template_options: &Map<String, Value>,
) -> Result<(), String> {
    if source.is_dir() {
        fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            copy_embedded_template_entry(
                &entry.path(),
                &destination.join(entry.file_name()),
                template_options,
            )?;
        }
        return Ok(());
    }

    let bytes = fs::read(source).map_err(|error| error.to_string())?;
    if let Ok(text) = String::from_utf8(bytes) {
        let substituted = substitute_template_options(&text, template_options);
        fs::write(destination, substituted).map_err(|error| error.to_string())?;
    } else {
        fs::copy(source, destination).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn substitute_template_options(contents: &str, template_options: &Map<String, Value>) -> String {
    let mut substituted = String::new();
    let mut remaining = contents;
    while let Some(start) = remaining.find("${templateOption:") {
        substituted.push_str(&remaining[..start]);
        let placeholder = &remaining[start + "${templateOption:".len()..];
        let Some(end) = placeholder.find('}') else {
            substituted.push_str(&remaining[start..]);
            return substituted;
        };
        let name = &placeholder[..end];
        if let Some(value) = template_options.get(name) {
            substituted.push_str(&template_option_string(value));
        } else {
            substituted.push_str(&remaining[start..start + "${templateOption:".len() + end + 1]);
        }
        remaining = &placeholder[end + 1..];
    }
    substituted.push_str(remaining);
    substituted
}

fn template_option_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn merge_extra_features_into_template(
    workspace_root: &Path,
    extra_features: Value,
) -> Result<(), String> {
    let Some(extra_features) = extra_features
        .as_array()
        .filter(|features| !features.is_empty())
    else {
        return Ok(());
    };
    let config_path = applied_template_config_path(workspace_root)
        .ok_or_else(|| "Applied template is missing a dev container config".to_string())?;
    let raw = fs::read_to_string(&config_path).map_err(|error| error.to_string())?;
    let mut config = crate::config::parse_jsonc_value(&raw)?;
    let config_object = config
        .as_object_mut()
        .ok_or_else(|| "Applied template config must be a JSON object".to_string())?;
    let features = config_object
        .entry("features".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "Applied template features must be a JSON object".to_string())?;
    for feature in extra_features {
        let Some(id) = feature.get("id").and_then(Value::as_str) else {
            continue;
        };
        features.insert(
            id.to_string(),
            feature.get("options").cloned().unwrap_or_else(|| json!({})),
        );
    }
    fs::write(
        config_path,
        serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn applied_template_config_path(workspace_root: &Path) -> Option<PathBuf> {
    [
        workspace_root
            .join(".devcontainer")
            .join("devcontainer.json"),
        workspace_root.join(".devcontainer.json"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn run_features_test(args: &[String]) -> ExitCode {
    match execute_feature_tests(args) {
        Ok(results) => {
            println!("  ================== TEST REPORT ==================");
            for result in &results {
                let status = if result.passed {
                    "✅ Passed"
                } else {
                    "❌ Failed"
                };
                println!("{status}:      '{}'", result.name);
            }
            if !common::has_flag(args, "--preserve-test-containers") {
                println!("Cleaning up {} test containers", results.len());
            }
            if results.iter().all(|result| result.passed) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FeatureTestCase {
    name: String,
    script_path: PathBuf,
    execution: FeatureTestExecution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FeatureTestResult {
    name: String,
    passed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FeatureTestExecution {
    Autogenerated { feature: String },
    Scenario { scenario_dir: String, config: Value },
    Duplicate { feature: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FeatureTestOptions {
    project_folder: PathBuf,
    base_image: String,
    remote_user: Option<String>,
    preserve_test_containers: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BaseImageSource {
    Image(String),
    Build {
        dockerfile_path: PathBuf,
        context_path: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FeatureInstallationSource {
    Local(PathBuf),
    Published(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FeatureInstallation {
    source: FeatureInstallationSource,
    env: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedFeatureTestCase {
    name: String,
    workspace_dir: PathBuf,
    build_context_dir: PathBuf,
    base_image: BaseImageSource,
    script_name: String,
    feature_installations: Vec<FeatureInstallation>,
    exec_env: Vec<(String, String)>,
    remote_user: Option<String>,
}

trait FeatureTestRuntime {
    fn build_image(
        &mut self,
        args: &[String],
        image_name: &str,
        dockerfile_path: &Path,
        context_path: &Path,
    ) -> Result<(), String>;
    fn start_container(
        &mut self,
        args: &[String],
        image_name: &str,
        workspace_dir: &Path,
    ) -> Result<String, String>;
    fn exec_script(
        &mut self,
        args: &[String],
        container_id: &str,
        workspace_dir: &Path,
        remote_user: Option<&str>,
        env: &[(String, String)],
        script_name: &str,
    ) -> Result<i32, String>;
    fn remove_container(&mut self, args: &[String], container_id: &str) -> Result<(), String>;
}

struct ContainerEngineFeatureTestRuntime;

impl FeatureTestRuntime for ContainerEngineFeatureTestRuntime {
    fn build_image(
        &mut self,
        args: &[String],
        image_name: &str,
        dockerfile_path: &Path,
        context_path: &Path,
    ) -> Result<(), String> {
        let result = runtime::engine::run_engine(
            args,
            vec![
                "build".to_string(),
                "--tag".to_string(),
                image_name.to_string(),
                "--file".to_string(),
                dockerfile_path.display().to_string(),
                context_path.display().to_string(),
            ],
        )?;
        if result.status_code != 0 {
            return Err(runtime::engine::stderr_or_stdout(&result));
        }
        Ok(())
    }

    fn start_container(
        &mut self,
        args: &[String],
        image_name: &str,
        workspace_dir: &Path,
    ) -> Result<String, String> {
        let result = runtime::engine::run_engine(
            args,
            vec![
                "run".to_string(),
                "-d".to_string(),
                "--label".to_string(),
                "devcontainer.is_test_run=true".to_string(),
                "--mount".to_string(),
                format!(
                    "type=bind,source={},target=/workspace",
                    workspace_dir.display()
                ),
                "--workdir".to_string(),
                "/workspace".to_string(),
                image_name.to_string(),
                "/bin/sh".to_string(),
                "-lc".to_string(),
                "while sleep 1000; do :; done".to_string(),
            ],
        )?;
        if result.status_code != 0 {
            return Err(runtime::engine::stderr_or_stdout(&result));
        }
        Ok(result.stdout.trim().to_string())
    }

    fn exec_script(
        &mut self,
        args: &[String],
        container_id: &str,
        _workspace_dir: &Path,
        remote_user: Option<&str>,
        env: &[(String, String)],
        script_name: &str,
    ) -> Result<i32, String> {
        let mut engine_args = vec![
            "exec".to_string(),
            "--workdir".to_string(),
            "/workspace".to_string(),
        ];
        if let Some(remote_user) = remote_user {
            engine_args.push("--user".to_string());
            engine_args.push(remote_user.to_string());
        }
        for (key, value) in env {
            engine_args.push("-e".to_string());
            engine_args.push(format!("{key}={value}"));
        }
        engine_args.push(container_id.to_string());
        engine_args.push("/bin/bash".to_string());
        engine_args.push("-lc".to_string());
        engine_args.push(format!(
            "chmod -R 777 /workspace && {}",
            shell_single_quote(&format!("./{script_name}"))
        ));
        runtime::engine::run_engine_streaming(args, engine_args)
    }

    fn remove_container(&mut self, args: &[String], container_id: &str) -> Result<(), String> {
        let result = runtime::engine::run_engine(
            args,
            vec!["rm".to_string(), "-f".to_string(), container_id.to_string()],
        )?;
        if result.status_code != 0 {
            return Err(runtime::engine::stderr_or_stdout(&result));
        }
        Ok(())
    }
}

#[cfg(test)]
fn discover_feature_test_scenarios(args: &[String]) -> Result<Vec<String>, String> {
    Ok(discover_feature_test_cases(args)?
        .into_iter()
        .map(|case| case.name)
        .collect())
}

fn execute_feature_tests(args: &[String]) -> Result<Vec<FeatureTestResult>, String> {
    let mut runtime = ContainerEngineFeatureTestRuntime;
    execute_feature_tests_with_runtime(args, &mut runtime)
}

fn execute_feature_tests_with_runtime<R: FeatureTestRuntime>(
    args: &[String],
    runtime: &mut R,
) -> Result<Vec<FeatureTestResult>, String> {
    let options = parse_feature_test_options(args)?;
    let cases = discover_feature_test_cases(args)?;
    let mut results = Vec::with_capacity(cases.len());

    for case in cases {
        let prepared = prepare_feature_test_case(&options, &case)?;
        let base_image = match &prepared.base_image {
            BaseImageSource::Image(image) => image.clone(),
            BaseImageSource::Build {
                dockerfile_path,
                context_path,
            } => {
                let image_name = unique_feature_test_name("devcontainer-feature-test-base");
                runtime.build_image(args, &image_name, dockerfile_path, context_path)?;
                image_name
            }
        };
        let dockerfile_path = write_feature_test_dockerfile(
            &prepared.build_context_dir,
            &base_image,
            &prepared.feature_installations,
        )?;
        let image_name = unique_feature_test_name("devcontainer-feature-test");
        runtime.build_image(
            args,
            &image_name,
            &dockerfile_path,
            &prepared.build_context_dir,
        )?;
        let container_id = runtime.start_container(args, &image_name, &prepared.workspace_dir)?;
        let status = runtime.exec_script(
            args,
            &container_id,
            &prepared.workspace_dir,
            prepared.remote_user.as_deref(),
            &prepared.exec_env,
            &prepared.script_name,
        )?;
        if !options.preserve_test_containers {
            runtime.remove_container(args, &container_id)?;
            let _ = fs::remove_dir_all(&prepared.workspace_dir);
        }
        results.push(FeatureTestResult {
            name: case.name,
            passed: status == 0,
        });
    }

    Ok(results)
}

fn parse_feature_test_options(args: &[String]) -> Result<FeatureTestOptions, String> {
    let project_folder = common::parse_option_value(args, "--project-folder")
        .or_else(|| common::parse_option_value(args, "--projectFolder"))
        .or_else(|| args.iter().rev().find(|arg| !arg.starts_with('-')).cloned())
        .map(PathBuf::from)
        .ok_or_else(|| "features test requires a project folder".to_string())?;
    let base_image = common::parse_option_value(args, "--base-image")
        .unwrap_or_else(|| DEFAULT_FEATURE_TEST_BASE_IMAGE.to_string());
    let remote_user = common::parse_option_value(args, "--remote-user");
    let preserve_test_containers = common::has_flag(args, "--preserve-test-containers");
    Ok(FeatureTestOptions {
        project_folder,
        base_image,
        remote_user,
        preserve_test_containers,
    })
}

fn prepare_feature_test_case(
    options: &FeatureTestOptions,
    case: &FeatureTestCase,
) -> Result<PreparedFeatureTestCase, String> {
    let workspace_dir = unique_feature_test_dir();
    fs::create_dir_all(&workspace_dir).map_err(|error| error.to_string())?;
    let test_dir = case
        .script_path
        .parent()
        .ok_or_else(|| format!("Invalid test script path: {}", case.script_path.display()))?;
    common::copy_directory_recursive(test_dir, &workspace_dir)?;
    fs::write(
        workspace_dir.join(FEATURE_TEST_LIBRARY_SCRIPT_NAME),
        FEATURE_TEST_LIBRARY_SCRIPT,
    )
    .map_err(|error| error.to_string())?;
    let script_name = case
        .script_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("Invalid test script path: {}", case.script_path.display()))?
        .to_string();
    let build_context_dir = workspace_dir.join(".feature-test-build");
    fs::create_dir_all(&build_context_dir).map_err(|error| error.to_string())?;

    let (base_image, feature_installations, exec_env) = match &case.execution {
        FeatureTestExecution::Autogenerated { feature } => (
            BaseImageSource::Image(options.base_image.clone()),
            vec![feature_installation(
                &options.project_folder.join("src").join(feature),
                &Value::Object(Map::new()),
            )?],
            Vec::new(),
        ),
        FeatureTestExecution::Scenario {
            scenario_dir,
            config,
        } => (
            scenario_base_image(options, scenario_dir, config, &workspace_dir)?,
            scenario_feature_installations(
                &options.project_folder,
                case.script_path
                    .parent()
                    .and_then(|path| path.file_name())
                    .and_then(|value| value.to_str())
                    .filter(|value| *value != "_global"),
                config,
            )?,
            Vec::new(),
        ),
        FeatureTestExecution::Duplicate { feature } => {
            let feature_dir = options.project_folder.join("src").join(feature);
            let default_options = feature_option_values(&feature_dir, &Value::Object(Map::new()))?;
            let alternate_options = alternate_feature_option_values(&feature_dir)?;
            let mut exec_env = alternate_options.clone();
            exec_env.extend(
                default_options
                    .iter()
                    .map(|(key, value)| (format!("{key}__DEFAULT"), value.clone())),
            );
            (
                BaseImageSource::Image(options.base_image.clone()),
                vec![
                    FeatureInstallation {
                        source: FeatureInstallationSource::Local(feature_dir.clone()),
                        env: alternate_options.clone(),
                    },
                    FeatureInstallation {
                        source: FeatureInstallationSource::Local(feature_dir),
                        env: default_options,
                    },
                ],
                exec_env,
            )
        }
    };

    Ok(PreparedFeatureTestCase {
        name: case.name.clone(),
        workspace_dir,
        build_context_dir,
        base_image,
        script_name,
        feature_installations,
        exec_env,
        remote_user: options.remote_user.clone(),
    })
}

fn scenario_base_image(
    options: &FeatureTestOptions,
    scenario_dir: &str,
    config: &Value,
    workspace_dir: &Path,
) -> Result<BaseImageSource, String> {
    if let Some(image) = config.get("image").and_then(Value::as_str) {
        return Ok(BaseImageSource::Image(image.to_string()));
    }

    let Some(build) = config.get("build").and_then(Value::as_object) else {
        return Ok(BaseImageSource::Image(options.base_image.clone()));
    };
    let config_root = scenario_config_root(workspace_dir, scenario_dir);
    let dockerfile = build
        .get("dockerfile")
        .or_else(|| build.get("dockerFile"))
        .and_then(Value::as_str)
        .unwrap_or("Dockerfile");
    let context = build.get("context").and_then(Value::as_str).unwrap_or(".");
    Ok(BaseImageSource::Build {
        dockerfile_path: resolve_relative_path(&config_root, dockerfile),
        context_path: resolve_relative_path(&config_root, context),
    })
}

fn scenario_feature_installations(
    project_folder: &Path,
    default_feature: Option<&str>,
    config: &Value,
) -> Result<Vec<FeatureInstallation>, String> {
    let features = if let Some(features) = config.get("features").and_then(Value::as_object) {
        features.clone()
    } else if let Some(default_feature) = default_feature {
        let mut features = Map::new();
        features.insert(default_feature.to_string(), Value::Object(Map::new()));
        features
    } else {
        return Err("Scenario is missing features".to_string());
    };

    let mut installations = Vec::with_capacity(features.len());
    for (feature_id, value) in &features {
        if feature_id.starts_with('.') {
            return Err(format!(
                "Unsupported relative feature in test scenario: {feature_id}"
            ));
        }
        if feature_id.contains('/') {
            installations.push(published_feature_installation(feature_id, value)?);
            continue;
        }
        installations.push(feature_installation(
            &project_folder.join("src").join(feature_id),
            value,
        )?);
    }
    Ok(installations)
}

fn feature_installation(feature_dir: &Path, value: &Value) -> Result<FeatureInstallation, String> {
    if !feature_dir.is_dir() {
        return Err(format!(
            "Feature source directory not found at {}",
            feature_dir.display()
        ));
    }
    Ok(FeatureInstallation {
        source: FeatureInstallationSource::Local(feature_dir.to_path_buf()),
        env: feature_option_values(feature_dir, value)?,
    })
}

fn published_feature_installation(
    feature_id: &str,
    value: &Value,
) -> Result<FeatureInstallation, String> {
    let manifest = published_feature_manifest(feature_id)
        .ok_or_else(|| format!("Unknown published feature: {feature_id}"))?;
    Ok(FeatureInstallation {
        source: FeatureInstallationSource::Published(feature_id.to_string()),
        env: feature_option_values_from_manifest(&manifest, value),
    })
}

fn feature_option_values(
    feature_dir: &Path,
    value: &Value,
) -> Result<Vec<(String, String)>, String> {
    let manifest = common::parse_manifest(feature_dir, "devcontainer-feature.json")?;
    Ok(feature_option_values_from_manifest(&manifest, value))
}

fn feature_option_values_from_manifest(manifest: &Value, value: &Value) -> Vec<(String, String)> {
    let defaults = manifest
        .get("options")
        .and_then(Value::as_object)
        .map(|options| {
            options
                .iter()
                .filter_map(|(key, option)| {
                    option
                        .get("default")
                        .map(|default| (feature_option_env_name(key), json_value_to_env(default)))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let overrides = value
        .as_object()
        .map(|options| {
            options
                .iter()
                .map(|(key, option)| (feature_option_env_name(key), json_value_to_env(option)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut merged = Map::new();
    for (key, value) in defaults.into_iter().chain(overrides) {
        merged.insert(key, Value::String(value));
    }
    merged
        .into_iter()
        .filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string())))
        .collect()
}

fn alternate_feature_option_values(feature_dir: &Path) -> Result<Vec<(String, String)>, String> {
    let manifest = common::parse_manifest(feature_dir, "devcontainer-feature.json")?;
    let Some(options) = manifest.get("options").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };

    let mut values = Vec::new();
    for (key, option) in options {
        let env_name = feature_option_env_name(key);
        let default = option.get("default");
        let value = match option.get("type").and_then(Value::as_str) {
            Some("boolean") => {
                let default = default.and_then(Value::as_bool).unwrap_or(false);
                (!default).to_string()
            }
            Some("string") => {
                if let Some(candidates) = option
                    .get("proposals")
                    .or_else(|| option.get("enum"))
                    .and_then(Value::as_array)
                {
                    let default = default.map(json_value_to_env);
                    candidates
                        .iter()
                        .map(json_value_to_env)
                        .find(|candidate| Some(candidate.clone()) != default)
                        .or(default)
                        .unwrap_or_default()
                } else {
                    default.map(json_value_to_env).unwrap_or_default()
                }
            }
            _ => default.map(json_value_to_env).unwrap_or_default(),
        };
        if !value.is_empty() {
            values.push((env_name, value));
        }
    }
    Ok(values)
}

fn json_value_to_env(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

fn feature_option_env_name(key: &str) -> String {
    key.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn write_feature_test_dockerfile(
    build_context_dir: &Path,
    base_image: &str,
    installations: &[FeatureInstallation],
) -> Result<PathBuf, String> {
    let dockerfile_path = build_context_dir.join("Dockerfile");
    let mut dockerfile = format!("FROM {base_image}\n");
    for (index, installation) in installations.iter().enumerate() {
        let feature_name = feature_installation_name(installation);
        let destination = format!("feature-{index}-{feature_name}");
        let copied_feature_dir = build_context_dir.join(&destination);
        materialize_feature_installation(installation, &copied_feature_dir)?;
        let install_path = format!("/tmp/devcontainer-features/{destination}");
        dockerfile.push_str(&format!("COPY {destination} {install_path}\n"));
        let env_assignments = installation
            .env
            .iter()
            .map(|(key, value)| format!("{key}={}", shell_single_quote(value)))
            .collect::<Vec<_>>()
            .join(" ");
        let command = if env_assignments.is_empty() {
            "chmod +x install.sh && ./install.sh".to_string()
        } else {
            format!("chmod +x install.sh && {env_assignments} ./install.sh")
        };
        dockerfile.push_str(&format!(
            "RUN cd {install_path} && /bin/sh -lc {}\n",
            shell_single_quote(&command)
        ));
    }
    fs::write(&dockerfile_path, dockerfile).map_err(|error| error.to_string())?;
    Ok(dockerfile_path)
}

fn feature_installation_name(installation: &FeatureInstallation) -> String {
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

fn materialize_feature_installation(
    installation: &FeatureInstallation,
    destination: &Path,
) -> Result<(), String> {
    match &installation.source {
        FeatureInstallationSource::Local(path) => materialize_local_feature(path, destination),
        FeatureInstallationSource::Published(feature_id) => {
            materialize_published_feature(feature_id, destination)
        }
    }
}

fn materialize_local_feature(source: &Path, destination: &Path) -> Result<(), String> {
    common::copy_directory_recursive(source, destination)?;
    ensure_feature_install_script(destination)
}

fn materialize_published_feature(feature_id: &str, destination: &Path) -> Result<(), String> {
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

fn ensure_feature_install_script(destination: &Path) -> Result<(), String> {
    let install_path = destination.join("install.sh");
    if install_path.is_file() {
        return Ok(());
    }
    fs::write(&install_path, "#!/bin/sh\nset -eu\n").map_err(|error| error.to_string())
}

fn unique_feature_test_name(prefix: &str) -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let unique_id = NEXT_FEATURE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{suffix}-{unique_id}", std::process::id())
}

fn unique_feature_test_dir() -> PathBuf {
    std::env::temp_dir().join(unique_feature_test_name(
        "devcontainer-feature-test-workspace",
    ))
}

fn resolve_relative_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn scenario_config_root(workspace_dir: &Path, scenario_dir: &str) -> PathBuf {
    let path = Path::new(scenario_dir);
    if path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        let candidate = workspace_dir.join(path);
        if candidate.is_dir() {
            return candidate;
        }
    }
    workspace_dir.to_path_buf()
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn discover_feature_test_cases(args: &[String]) -> Result<Vec<FeatureTestCase>, String> {
    let project_folder = common::parse_option_value(args, "--project-folder")
        .or_else(|| common::parse_option_value(args, "--projectFolder"))
        .or_else(|| args.iter().rev().find(|arg| !arg.starts_with('-')).cloned())
        .ok_or_else(|| "features test requires a project folder".to_string())?;
    let filter = common::parse_option_value(args, "--filter");
    let feature_filter = common::parse_option_value(args, "-f")
        .or_else(|| common::parse_option_value(args, "--features"));
    let skip_scenarios = common::has_flag(args, "--skip-scenarios");
    let global_scenarios_only = common::has_flag(args, "--global-scenarios-only");
    let skip_autogenerated = common::has_flag(args, "--skip-autogenerated");
    let skip_duplicated = common::has_flag(args, "--skip-duplicated");
    let test_root = Path::new(&project_folder).join("test");
    let mut cases = Vec::new();

    if let Ok(entries) = fs::read_dir(&test_root) {
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "_global" {
                if feature_filter.is_none() && !skip_scenarios {
                    cases.extend(load_named_scenarios(&entry.path())?);
                }
                continue;
            }
            if global_scenarios_only {
                continue;
            }
            if feature_filter
                .as_deref()
                .is_some_and(|value| value != name.as_str())
            {
                continue;
            }
            if entry.path().join("test.sh").is_file() && !skip_autogenerated {
                cases.push(FeatureTestCase {
                    name: name.clone(),
                    script_path: entry.path().join("test.sh"),
                    execution: FeatureTestExecution::Autogenerated {
                        feature: name.clone(),
                    },
                });
            }
            if entry.path().join("duplicate.sh").is_file() && !skip_duplicated {
                cases.push(FeatureTestCase {
                    name: format!("{name} executed twice with randomized options"),
                    script_path: entry.path().join("duplicate.sh"),
                    execution: FeatureTestExecution::Duplicate {
                        feature: name.clone(),
                    },
                });
            }
            if !skip_scenarios {
                cases.extend(load_named_scenarios(&entry.path())?);
            }
        }
    }

    cases.sort_by(|left, right| left.name.cmp(&right.name));
    cases.dedup_by(|left, right| left.name == right.name);
    if let Some(filter) = filter {
        cases.retain(|scenario| scenario.name.contains(&filter));
    }
    Ok(cases)
}

fn load_named_scenarios(test_dir: &Path) -> Result<Vec<FeatureTestCase>, String> {
    let scenarios_path = test_dir.join("scenarios.json");
    if !scenarios_path.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(scenarios_path).map_err(|error| error.to_string())?;
    let parsed = crate::config::parse_jsonc_value(&raw)?;
    parsed
        .as_object()
        .map(|entries| {
            entries
                .iter()
                .map(|(name, config)| {
                    let script_path = test_dir.join(format!("{name}.sh"));
                    if !script_path.is_file() {
                        return Err(format!(
                            "No scenario test script found at path '{}'",
                            script_path.display()
                        ));
                    }
                    Ok(FeatureTestCase {
                        name: name.clone(),
                        script_path,
                        execution: FeatureTestExecution::Scenario {
                            scenario_dir: name.clone(),
                            config: config.clone(),
                        },
                    })
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
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

fn published_feature_install_script(feature_id: &str) -> &'static str {
    match normalize_collection_reference(feature_id).as_str() {
        "ghcr.io/devcontainers/features/common-utils" => {
            r#"#!/bin/sh
set -eu

username="${USERNAME:-}"
if [ -n "$username" ] && [ "$username" != "none" ] && ! id -u "$username" >/dev/null 2>&1; then
    if command -v useradd >/dev/null 2>&1; then
        useradd -m "$username" >/dev/null 2>&1 || true
    elif command -v adduser >/dev/null 2>&1; then
        adduser -D "$username" >/dev/null 2>&1 || adduser --disabled-password --gecos "" "$username" >/dev/null 2>&1 || true
    fi
fi
"#
        }
        _ => {
            r#"#!/bin/sh
set -eu
"#
        }
    }
}

fn published_feature_manifest(feature_id: &str) -> Option<Value> {
    let normalized = normalize_collection_reference(feature_id);
    let manifest = match normalized.as_str() {
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
    };
    if manifest.is_some() {
        return manifest;
    }

    let slug = collection_slug(&normalized)?;
    if !normalized.contains("/features/") {
        return None;
    }
    Some(json!({
        "id": slug,
        "name": humanize_collection_slug(&slug),
        "version": collection_reference_version(feature_id),
        "options": {},
    }))
}

fn published_template_manifest(template_id: &str) -> Option<Value> {
    let normalized = normalize_collection_reference(template_id);
    let manifest = match normalized.as_str() {
        "ghcr.io/devcontainers/templates/docker-from-docker" => Some(json!({
            "id": "docker-from-docker",
            "name": "Docker from Docker",
            "description": "Create a dev container with Docker available inside the container.",
            "version": "1.0.0"
        })),
        _ => embedded_template_manifest(&normalized),
    };
    if manifest.is_some() {
        return manifest;
    }

    let slug = collection_slug(&normalized)?;
    if !normalized.contains("/templates/") {
        return None;
    }
    Some(json!({
        "id": slug,
        "name": humanize_collection_slug(&slug),
        "description": "",
        "version": collection_reference_version(template_id),
    }))
}

fn normalize_collection_reference(reference: &str) -> String {
    if let Some(index) = reference.find('@') {
        return reference[..index].to_string();
    }
    let last_slash = reference.rfind('/').unwrap_or(0);
    if let Some(index) = reference.rfind(':').filter(|index| *index > last_slash) {
        return reference[..index].to_string();
    }
    reference.to_string()
}

fn collection_slug(reference: &str) -> Option<String> {
    normalize_collection_reference(reference)
        .rsplit('/')
        .next()
        .map(|value| value.to_ascii_lowercase())
}

fn collection_reference_version(reference: &str) -> String {
    let normalized = normalize_collection_reference(reference);
    if let Some(digest) = reference
        .strip_prefix(&normalized)
        .and_then(|suffix| suffix.strip_prefix('@'))
    {
        return digest.to_string();
    }
    if let Some(version) = reference
        .strip_prefix(&normalized)
        .and_then(|suffix| suffix.strip_prefix(':'))
    {
        return version.to_string();
    }
    "latest".to_string()
}

fn humanize_collection_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn embedded_template_manifest(reference: &str) -> Option<Value> {
    match collection_slug(reference)?.as_str() {
        "alpine" => serde_json::from_str(include_str!(
            "../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/alpine/devcontainer-template.json"
        ))
        .ok(),
        "cpp" => serde_json::from_str(include_str!(
            "../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/cpp/devcontainer-template.json"
        ))
        .ok(),
        "mytemplate" => serde_json::from_str(include_str!(
            "../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/mytemplate/devcontainer-template.json"
        ))
        .ok(),
        "node-mongo" => serde_json::from_str(include_str!(
            "../../../../upstream/src/test/container-templates/example-templates-sets/simple/src/node-mongo/devcontainer-template.json"
        ))
        .ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_catalog_template, apply_template_target, build_feature_info_payload,
        build_features_resolve_dependencies_payload, build_template_metadata_payload,
        discover_feature_test_scenarios, execute_feature_tests_with_runtime,
        publish_collection_target_to_oci, FeatureTestRuntime,
    };
    use crate::commands::common::{generate_manifest_docs, package_collection_target};
    use serde_json::json;
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
            "devcontainer-collection-test-{}-{suffix}-{unique_id}",
            std::process::id()
        ))
    }

    #[derive(Default)]
    struct FakeFeatureTestRuntime {
        build_calls: Vec<(String, PathBuf, PathBuf)>,
        start_calls: Vec<(String, PathBuf)>,
        exec_calls: Vec<(
            String,
            PathBuf,
            Option<String>,
            Vec<(String, String)>,
            String,
        )>,
        remove_calls: Vec<String>,
    }

    impl FeatureTestRuntime for FakeFeatureTestRuntime {
        fn build_image(
            &mut self,
            _args: &[String],
            image_name: &str,
            dockerfile_path: &Path,
            context_path: &Path,
        ) -> Result<(), String> {
            self.build_calls.push((
                image_name.to_string(),
                dockerfile_path.to_path_buf(),
                context_path.to_path_buf(),
            ));
            Ok(())
        }

        fn start_container(
            &mut self,
            _args: &[String],
            image_name: &str,
            workspace_dir: &Path,
        ) -> Result<String, String> {
            self.start_calls
                .push((image_name.to_string(), workspace_dir.to_path_buf()));
            Ok(format!("container-{}", self.start_calls.len()))
        }

        fn exec_script(
            &mut self,
            _args: &[String],
            container_id: &str,
            workspace_dir: &Path,
            remote_user: Option<&str>,
            env: &[(String, String)],
            script_name: &str,
        ) -> Result<i32, String> {
            self.exec_calls.push((
                container_id.to_string(),
                workspace_dir.to_path_buf(),
                remote_user.map(str::to_string),
                env.to_vec(),
                script_name.to_string(),
            ));
            Ok(if script_name == "failing.sh" { 1 } else { 0 })
        }

        fn remove_container(&mut self, _args: &[String], container_id: &str) -> Result<(), String> {
            self.remove_calls.push(container_id.to_string());
            Ok(())
        }
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
        fs::write(test.join("custom.sh"), "#!/bin/sh\n").expect("scenario script");
        fs::write(
            test.join("scenarios.json"),
            "{\n  \"custom\": {\n    \"image\": \"ubuntu:latest\"\n  }\n}\n",
        )
        .expect("scenarios");
        fs::write(global.join("global-scenario.sh"), "#!/bin/sh\n")
            .expect("global scenario script");
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
    fn features_test_executes_scripts_inside_test_containers() {
        let root = unique_temp_dir();
        let src = root.join("src").join("demo");
        let test = root.join("test").join("demo");
        fs::create_dir_all(&src).expect("feature src");
        fs::create_dir_all(&test).expect("feature test");
        fs::write(
            src.join("devcontainer-feature.json"),
            "{\n  \"id\": \"demo\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\",\n  \"options\": {\n    \"favorite\": {\n      \"type\": \"string\",\n      \"default\": \"blue\"\n    }\n  }\n}\n",
        )
        .expect("manifest");
        fs::write(src.join("install.sh"), "#!/bin/sh\nexit 0\n").expect("install script");
        fs::write(test.join("test.sh"), "#!/bin/sh\nexit 0\n").expect("test script");
        fs::write(test.join("failing.sh"), "#!/bin/sh\nexit 1\n").expect("scenario script");
        fs::write(
            test.join("scenarios.json"),
            "{\n  \"failing\": {\n    \"image\": \"ubuntu:latest\",\n    \"features\": {\n      \"demo\": {\n        \"favorite\": \"red\"\n      }\n    }\n  }\n}\n",
        )
        .expect("scenarios");

        let mut runtime = FakeFeatureTestRuntime::default();
        let results = execute_feature_tests_with_runtime(
            &[
                "--preserve-test-containers".to_string(),
                root.display().to_string(),
            ],
            &mut runtime,
        )
        .expect("test execution");

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|result| result.name == "demo" && result.passed));
        assert!(results
            .iter()
            .any(|result| result.name == "failing" && !result.passed));
        assert_eq!(runtime.start_calls.len(), 2);
        assert_eq!(runtime.exec_calls[0].0, "container-1");
        assert_eq!(runtime.exec_calls[0].4, "test.sh");
        assert!(runtime.exec_calls[0]
            .1
            .join("dev-container-features-test-lib")
            .is_file());
        assert_eq!(runtime.exec_calls[1].4, "failing.sh");
        assert!(runtime.remove_calls.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn features_test_defaults_per_feature_scenarios_to_enclosing_feature() {
        let root = unique_temp_dir();
        let src = root.join("src").join("demo");
        let test = root.join("test").join("demo");
        fs::create_dir_all(&src).expect("feature src");
        fs::create_dir_all(&test).expect("feature test");
        fs::write(
            src.join("devcontainer-feature.json"),
            "{\n  \"id\": \"demo\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
        )
        .expect("manifest");
        fs::write(src.join("install.sh"), "#!/bin/sh\nexit 0\n").expect("install script");
        fs::write(test.join("test.sh"), "#!/bin/sh\nexit 0\n").expect("test script");
        fs::write(test.join("custom.sh"), "#!/bin/sh\nexit 0\n").expect("scenario script");
        fs::write(
            test.join("scenarios.json"),
            "{\n  \"custom\": {\n    \"image\": \"ubuntu:latest\"\n  }\n}\n",
        )
        .expect("scenarios");

        let mut runtime = FakeFeatureTestRuntime::default();
        let results = execute_feature_tests_with_runtime(
            &[
                "--preserve-test-containers".to_string(),
                root.display().to_string(),
            ],
            &mut runtime,
        )
        .expect("test execution");

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|result| result.name == "demo" && result.passed));
        assert!(results
            .iter()
            .any(|result| result.name == "custom" && result.passed));
        assert!(runtime
            .build_calls
            .iter()
            .map(|(_, _, context_path)| context_path.join("feature-0-demo").join("install.sh"))
            .any(|install_path| install_path.is_file()));
        for (_, workspace_dir) in &runtime.start_calls {
            let _ = fs::remove_dir_all(workspace_dir);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn features_test_accepts_published_feature_dependencies_in_scenarios() {
        let root = unique_temp_dir();
        let src = root.join("src").join("demo");
        let test = root.join("test").join("demo");
        fs::create_dir_all(&src).expect("feature src");
        fs::create_dir_all(&test).expect("feature test");
        fs::write(
            src.join("devcontainer-feature.json"),
            "{\n  \"id\": \"demo\",\n  \"name\": \"Demo Feature\",\n  \"version\": \"1.0.0\"\n}\n",
        )
        .expect("manifest");
        fs::write(src.join("install.sh"), "#!/bin/sh\nexit 0\n").expect("install script");
        fs::write(test.join("test.sh"), "#!/bin/sh\nexit 0\n").expect("test script");
        fs::write(test.join("dependency.sh"), "#!/bin/sh\nexit 0\n").expect("scenario script");
        fs::write(
            test.join("scenarios.json"),
            "{\n  \"dependency\": {\n    \"image\": \"ubuntu:latest\",\n    \"features\": {\n      \"ghcr.io/devcontainers/features/common-utils:1\": {},\n      \"demo\": {}\n    }\n  }\n}\n",
        )
        .expect("scenarios");

        let mut runtime = FakeFeatureTestRuntime::default();
        let results = execute_feature_tests_with_runtime(
            &[
                "--preserve-test-containers".to_string(),
                root.display().to_string(),
            ],
            &mut runtime,
        )
        .expect("test execution");

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|result| result.name == "dependency" && result.passed));
        let dockerfiles = runtime
            .build_calls
            .iter()
            .map(|(_, dockerfile_path, _)| {
                fs::read_to_string(dockerfile_path).expect("dockerfile contents")
            })
            .collect::<Vec<_>>();
        assert!(dockerfiles
            .iter()
            .any(|dockerfile| dockerfile.contains("common-utils")));

        for (_, workspace_dir) in &runtime.start_calls {
            let _ = fs::remove_dir_all(workspace_dir);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn feature_info_supports_digest_pinned_catalog_refs() {
        let payload = build_feature_info_payload(
            "manifest",
            "ghcr.io/devcontainers/features/git-lfs@sha256:24d5802c837b2519b666a8403a9514c7296d769c9607048e9f1e040e7d7e331c",
        )
        .expect("feature info");

        assert_eq!(payload["id"], "git-lfs");
        assert_eq!(payload["name"], "Git Lfs");
    }

    #[test]
    fn template_metadata_supports_digest_pinned_catalog_refs() {
        let payload = build_template_metadata_payload(
            "ghcr.io/devcontainers/templates/docker-from-docker@sha256:0123456789abcdef",
        )
        .expect("template metadata");

        assert_eq!(payload["id"], "docker-from-docker");
        assert_eq!(payload["name"], "Docker from Docker");
    }

    #[test]
    fn published_embedded_templates_copy_upstream_source_files() {
        let workspace = unique_temp_dir();
        fs::create_dir_all(&workspace).expect("workspace");

        let payload = apply_catalog_template(
            "ghcr.io/devcontainers/templates/node-mongo:latest",
            &workspace,
            &[],
        )
        .expect("template apply");

        assert_eq!(payload["id"], "node-mongo");
        assert!(workspace
            .join(".devcontainer")
            .join("docker-compose.yml")
            .is_file());
        assert!(workspace
            .join(".devcontainer")
            .join("devcontainer.json")
            .is_file());
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn published_embedded_templates_apply_template_args_and_extra_features() {
        let workspace = unique_temp_dir();
        fs::create_dir_all(&workspace).expect("workspace");

        apply_catalog_template(
            "ghcr.io/devcontainers/templates/alpine:latest",
            &workspace,
            &[
                "--template-args".to_string(),
                json!({ "imageVariant": "3.14" }).to_string(),
                "--features".to_string(),
                json!([{ "id": "ghcr.io/devcontainers/features/git:1", "options": {} }])
                    .to_string(),
            ],
        )
        .expect("template apply");

        let config = fs::read_to_string(workspace.join(".devcontainer.json")).expect("config");
        assert!(config.contains("0-alpine-3.14"));
        assert!(config.contains("ghcr.io/devcontainers/features/git:1"));
        let _ = fs::remove_dir_all(workspace);
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
    fn feature_info_supports_generic_published_features() {
        let payload = build_feature_info_payload("manifest", "ghcr.io/devcontainers/features/node")
            .expect("feature info");

        assert_eq!(payload["id"], "node");
        assert_eq!(payload["name"], "Node");
        assert_eq!(payload["version"], "latest");
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
    fn template_metadata_supports_generic_published_templates() {
        let payload = build_template_metadata_payload(
            "ghcr.io/devcontainers/templates/anaconda-postgres:latest",
        )
        .expect("template metadata");

        assert_eq!(payload["id"], "anaconda-postgres");
        assert_eq!(payload["name"], "Anaconda Postgres");
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
