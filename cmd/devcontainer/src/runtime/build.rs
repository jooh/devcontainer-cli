use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::commands::common;

use super::compose;
use super::context::ResolvedConfig;
use super::engine;

pub(crate) fn runtime_image_name(
    resolved: &ResolvedConfig,
    args: &[String],
) -> Result<String, String> {
    if compose::uses_compose_config(&resolved.configuration) {
        compose::build_service(resolved, args)
    } else if has_build_definition(&resolved.configuration) {
        build_image(resolved, args)
    } else if let Some(image) = resolved.configuration.get("image").and_then(Value::as_str) {
        Ok(image.to_string())
    } else {
        Err(
            "Unsupported configuration: only image and build-based configs are supported natively"
                .to_string(),
        )
    }
}

pub(crate) fn build_image(resolved: &ResolvedConfig, args: &[String]) -> Result<String, String> {
    if compose::uses_compose_config(&resolved.configuration) {
        return compose::build_service(resolved, args);
    }

    if !has_build_definition(&resolved.configuration) {
        return resolved
            .configuration
            .get("image")
            .and_then(Value::as_str)
            .map(|value| value.to_string())
            .ok_or_else(|| {
                "Unsupported configuration: only image and build-based configs are supported natively"
                    .to_string()
            });
    }

    let image_name = common::parse_option_value(args, "--image-name")
        .unwrap_or_else(|| default_image_name(&resolved.workspace_folder));
    let build = resolved
        .configuration
        .get("build")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let config_root = resolved
        .config_file
        .parent()
        .unwrap_or(resolved.workspace_folder.as_path());
    let dockerfile = build
        .get("dockerfile")
        .or_else(|| build.get("dockerFile"))
        .and_then(Value::as_str)
        .unwrap_or("Dockerfile");
    let context = build.get("context").and_then(Value::as_str).unwrap_or(".");
    let dockerfile_path = resolve_relative(config_root, dockerfile);
    let context_path = resolve_relative(config_root, context);

    let mut engine_args = vec![
        "build".to_string(),
        "--tag".to_string(),
        image_name.clone(),
        "--file".to_string(),
        dockerfile_path.display().to_string(),
    ];
    if common::has_flag(args, "--no-cache") || common::has_flag(args, "--build-no-cache") {
        engine_args.push("--no-cache".to_string());
    }
    for value in common::parse_option_values(args, "--cache-from") {
        engine_args.push("--cache-from".to_string());
        engine_args.push(value);
    }
    for value in common::parse_option_values(args, "--cache-to") {
        engine_args.push("--cache-to".to_string());
        engine_args.push(value);
    }
    for value in common::parse_option_values(args, "--label") {
        engine_args.push("--label".to_string());
        engine_args.push(value);
    }
    if let Some(build_args) = build.get("args").and_then(Value::as_object) {
        for (key, value) in build_args {
            if let Some(value) = value.as_str() {
                engine_args.push("--build-arg".to_string());
                engine_args.push(format!("{key}={value}"));
            }
        }
    }
    if let Some(platform) = common::parse_option_value(args, "--platform") {
        engine_args.push("--platform".to_string());
        engine_args.push(platform);
    }
    engine_args.push(context_path.display().to_string());

    let result = engine::run_engine(args, engine_args)?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    if common::has_flag(args, "--push") {
        let push_result = engine::run_engine(args, vec!["push".to_string(), image_name.clone()])?;
        if push_result.status_code != 0 {
            return Err(engine::stderr_or_stdout(&push_result));
        }
    }

    Ok(image_name)
}

fn default_image_name(workspace_folder: &Path) -> String {
    let basename = workspace_folder
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("devcontainer-{basename}")
}

fn has_build_definition(configuration: &Value) -> bool {
    configuration
        .get("build")
        .is_some_and(|value| value.is_object())
}

fn resolve_relative(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}
