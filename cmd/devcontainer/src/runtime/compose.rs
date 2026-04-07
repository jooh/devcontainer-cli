use std::path::{Path, PathBuf};

use serde_json::Value;
use serde_yaml::{Mapping, Value as YamlValue};

use crate::commands::common;

use super::context::ResolvedConfig;
use super::engine;

pub(crate) struct ComposeSpec {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) service: String,
    pub(crate) image: Option<String>,
    pub(crate) has_build: bool,
}

pub(crate) fn uses_compose_config(configuration: &Value) -> bool {
    configuration
        .get("dockerComposeFile")
        .is_some()
        && configuration.get("service").and_then(Value::as_str).is_some()
}

pub(crate) fn load_compose_spec(resolved: &ResolvedConfig) -> Result<Option<ComposeSpec>, String> {
    if !uses_compose_config(&resolved.configuration) {
        return Ok(None);
    }

    let config_root = resolved
        .config_file
        .parent()
        .unwrap_or(resolved.workspace_folder.as_path());
    let files = compose_files(&resolved.configuration, config_root)?;
    let service = resolved
        .configuration
        .get("service")
        .and_then(Value::as_str)
        .ok_or_else(|| "Compose configuration must define service".to_string())?
        .to_string();
    let (image, has_build) = inspect_service_definition(&files, &service)?;

    Ok(Some(ComposeSpec {
        files,
        service,
        image,
        has_build,
    }))
}

pub(crate) fn build_service(resolved: &ResolvedConfig, args: &[String]) -> Result<String, String> {
    let spec = load_compose_spec(resolved)?
        .ok_or_else(|| "Compose configuration was expected but not found".to_string())?;

    if spec.has_build {
        let result = engine::run_compose(args, compose_args(&spec.files, "build", &["--pull", &spec.service]))?;
        if result.status_code != 0 {
            return Err(engine::stderr_or_stdout(&result));
        }
    }

    if common::has_flag(args, "--push") {
        if let Some(image) = &spec.image {
            let push_result = engine::run_engine(args, vec!["push".to_string(), image.clone()])?;
            if push_result.status_code != 0 {
                return Err(engine::stderr_or_stdout(&push_result));
            }
        } else {
            return Err("Compose build push requires the service to declare an image".to_string());
        }
    }

    Ok(spec
        .image
        .unwrap_or_else(|| format!("compose-service-{}", spec.service)))
}

pub(crate) fn up_service(resolved: &ResolvedConfig, args: &[String]) -> Result<(), String> {
    let spec = load_compose_spec(resolved)?
        .ok_or_else(|| "Compose configuration was expected but not found".to_string())?;
    let result = engine::run_compose(args, compose_args(&spec.files, "up", &["-d", &spec.service]))?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

pub(crate) fn remove_service(resolved: &ResolvedConfig, args: &[String]) -> Result<(), String> {
    let spec = load_compose_spec(resolved)?
        .ok_or_else(|| "Compose configuration was expected but not found".to_string())?;
    let result = engine::run_compose(args, compose_args(&spec.files, "rm", &["-s", "-f", &spec.service]))?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

pub(crate) fn resolve_container_id(
    resolved: &ResolvedConfig,
    args: &[String],
) -> Result<Option<String>, String> {
    let spec = load_compose_spec(resolved)?
        .ok_or_else(|| "Compose configuration was expected but not found".to_string())?;
    let result = engine::run_compose(args, compose_args(&spec.files, "ps", &["-q", &spec.service]))?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }

    Ok(result
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.chars().any(char::is_whitespace))
        .map(str::to_string))
}

fn compose_args(files: &[PathBuf], subcommand: &str, tail: &[&str]) -> Vec<String> {
    let mut args = Vec::new();
    for file in files {
        args.push("-f".to_string());
        args.push(file.display().to_string());
    }
    args.push(subcommand.to_string());
    args.extend(tail.iter().map(|value| value.to_string()));
    args
}

fn compose_files(configuration: &Value, config_root: &Path) -> Result<Vec<PathBuf>, String> {
    match configuration.get("dockerComposeFile") {
        Some(Value::String(value)) => Ok(vec![resolve_relative(config_root, value)]),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value.as_str()
                    .map(|path| resolve_relative(config_root, path))
                    .ok_or_else(|| "dockerComposeFile entries must be strings".to_string())
            })
            .collect(),
        Some(_) => Err("dockerComposeFile must be a string or array of strings".to_string()),
        None => Err("Compose configuration must define dockerComposeFile".to_string()),
    }
}

fn inspect_service_definition(
    compose_files: &[PathBuf],
    service: &str,
) -> Result<(Option<String>, bool), String> {
    let mut image = None;
    let mut has_build = false;
    let mut found_service = false;

    for compose_file in compose_files {
        let raw = std::fs::read_to_string(compose_file).map_err(|error| error.to_string())?;
        let parsed: YamlValue = serde_yaml::from_str(&raw).map_err(|error| error.to_string())?;
        let Some(service_definition) = parsed
            .as_mapping()
            .and_then(|root| root.get(YamlValue::String("services".to_string())))
            .and_then(YamlValue::as_mapping)
            .and_then(|services| services.get(YamlValue::String(service.to_string())))
            .and_then(YamlValue::as_mapping)
        else {
            continue;
        };

        found_service = true;

        if service_definition.contains_key(YamlValue::String("build".to_string())) {
            has_build = true;
        }
        if let Some(value) = service_field(service_definition, "image").and_then(YamlValue::as_str) {
            image = Some(value.to_string());
        }
    }

    if !found_service {
        return Err(format!(
            "Unable to locate compose service `{service}` in compose configuration"
        ));
    }

    Ok((image, has_build))
}

fn service_field<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a YamlValue> {
    mapping.get(YamlValue::String(key.to_string()))
}

fn resolve_relative(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{inspect_service_definition, uses_compose_config};
    use serde_json::json;
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
            "devcontainer-compose-test-{}-{suffix}-{unique_id}",
            std::process::id()
        ))
    }

    #[test]
    fn detects_compose_configs() {
        assert!(uses_compose_config(&json!({
            "dockerComposeFile": "docker-compose.yml",
            "service": "app"
        })));
        assert!(!uses_compose_config(&json!({
            "image": "alpine:3.20"
        })));
    }

    #[test]
    fn inspects_service_image_and_build_presence() {
        let root = unique_temp_dir();
        let compose_file = root.join("docker-compose.yml");
        fs::create_dir_all(&root).expect("compose root");
        fs::write(
            &compose_file,
            "services:\n  app:\n    image: example/native-compose:test\n    build:\n      context: .\n",
        )
        .expect("compose file");

        let (image, has_build) =
            inspect_service_definition(&[compose_file], "app").expect("service definition");

        assert_eq!(image.as_deref(), Some("example/native-compose:test"));
        assert!(has_build);
        let _ = fs::remove_dir_all(root);
    }
}
