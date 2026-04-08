use std::env;
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
    pub(crate) project_name: String,
}

pub(crate) fn uses_compose_config(configuration: &Value) -> bool {
    configuration.get("dockerComposeFile").is_some()
        && configuration
            .get("service")
            .and_then(Value::as_str)
            .is_some()
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
    let project_name = compose_project_name(&files)?;
    let (image, has_build) = inspect_service_definition(&files, &service)?;

    Ok(Some(ComposeSpec {
        files,
        service,
        image,
        has_build,
        project_name,
    }))
}

pub(crate) fn build_service(resolved: &ResolvedConfig, args: &[String]) -> Result<String, String> {
    let spec = load_compose_spec(resolved)?
        .ok_or_else(|| "Compose configuration was expected but not found".to_string())?;
    reject_unsupported_build_options(args)?;

    if spec.has_build {
        let mut build_args = vec!["--pull".to_string()];
        if common::has_flag(args, "--no-cache") || common::has_flag(args, "--build-no-cache") {
            build_args.push("--no-cache".to_string());
        }
        build_args.push(spec.service.clone());
        let result = engine::run_compose(args, compose_args_owned(&spec, "build", build_args))?;
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
    let result = engine::run_compose(args, compose_args(&spec, "up", &["-d", &spec.service]))?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

pub(crate) fn remove_service(resolved: &ResolvedConfig, args: &[String]) -> Result<(), String> {
    let spec = load_compose_spec(resolved)?
        .ok_or_else(|| "Compose configuration was expected but not found".to_string())?;
    let result = engine::run_compose(
        args,
        compose_args(&spec, "rm", &["-s", "-f", &spec.service]),
    )?;
    if result.status_code != 0 {
        return Err(engine::stderr_or_stdout(&result));
    }
    Ok(())
}

pub(crate) fn resolve_container_id(
    resolved: &ResolvedConfig,
    args: &[String],
) -> Result<Option<String>, String> {
    resolve_container_id_with_options(resolved, args, false)
}

pub(crate) fn resolve_container_id_including_stopped(
    resolved: &ResolvedConfig,
    args: &[String],
) -> Result<Option<String>, String> {
    resolve_container_id_with_options(resolved, args, true)
}

fn resolve_container_id_with_options(
    resolved: &ResolvedConfig,
    args: &[String],
    include_stopped: bool,
) -> Result<Option<String>, String> {
    let spec = load_compose_spec(resolved)?
        .ok_or_else(|| "Compose configuration was expected but not found".to_string())?;
    let mut ps_args = vec!["-q"];
    if include_stopped {
        ps_args.push("-a");
    }
    ps_args.push(&spec.service);
    let result = engine::run_compose(args, compose_args(&spec, "ps", &ps_args))?;
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

fn compose_args(spec: &ComposeSpec, subcommand: &str, tail: &[&str]) -> Vec<String> {
    compose_args_owned(
        spec,
        subcommand,
        tail.iter().map(|value| value.to_string()).collect(),
    )
}

fn compose_args_owned(spec: &ComposeSpec, subcommand: &str, tail: Vec<String>) -> Vec<String> {
    let mut args = Vec::new();
    args.push("--project-name".to_string());
    args.push(spec.project_name.clone());
    for file in &spec.files {
        args.push("-f".to_string());
        args.push(file.display().to_string());
    }
    args.push(subcommand.to_string());
    args.extend(tail);
    args
}

fn reject_unsupported_build_options(args: &[String]) -> Result<(), String> {
    for flag in ["--cache-from", "--cache-to", "--platform", "--label"] {
        if compose_build_option_is_present(args, flag) {
            return Err(format!("{flag} not supported for compose builds."));
        }
    }
    Ok(())
}

fn compose_build_option_is_present(args: &[String], flag: &str) -> bool {
    common::has_flag(args, flag)
        || common::parse_option_value(args, flag).is_some()
        || !common::parse_option_values(args, flag).is_empty()
}

fn compose_files(configuration: &Value, config_root: &Path) -> Result<Vec<PathBuf>, String> {
    match configuration.get("dockerComposeFile") {
        Some(Value::String(value)) => Ok(vec![resolve_relative(config_root, value)]),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
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
        if let Some(value) = service_field(service_definition, "image").and_then(YamlValue::as_str)
        {
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

fn compose_project_name(compose_files: &[PathBuf]) -> Result<String, String> {
    if let Some(value) = env::var("COMPOSE_PROJECT_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(sanitize_project_name(&value));
    }
    if let Some(value) = compose_project_name_from_dotenv(compose_files)? {
        return Ok(sanitize_project_name(&value));
    }
    for compose_file in compose_files.iter().rev() {
        if let Some(value) = compose_name_from_file(compose_file)? {
            return Ok(sanitize_project_name(&value));
        }
    }

    let working_dir = compose_files
        .first()
        .and_then(|file| file.parent())
        .ok_or_else(|| "Compose configuration must define at least one compose file".to_string())?;
    let base = if working_dir.file_name().and_then(|value| value.to_str()) == Some(".devcontainer")
    {
        format!(
            "{}_devcontainer",
            working_dir
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .unwrap_or("devcontainer")
        )
    } else {
        working_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("devcontainer")
            .to_string()
    };
    Ok(sanitize_project_name(&base))
}

fn compose_project_name_from_dotenv(compose_files: &[PathBuf]) -> Result<Option<String>, String> {
    let env_file = compose_files
        .first()
        .and_then(|file| file.parent())
        .ok_or_else(|| "Compose configuration must define at least one compose file".to_string())?
        .join(".env");
    let raw = match std::fs::read_to_string(env_file) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    Ok(raw.lines().find_map(|line| {
        line.trim()
            .strip_prefix("COMPOSE_PROJECT_NAME=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }))
}

fn compose_name_from_file(compose_file: &Path) -> Result<Option<String>, String> {
    let raw = std::fs::read_to_string(compose_file).map_err(|error| error.to_string())?;
    Ok(raw.lines().find_map(|line| {
        if line.starts_with(' ') || line.starts_with('\t') {
            return None;
        }
        line.strip_prefix("name:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(substitute_compose_env)
    }))
}

fn substitute_compose_env(value: &str) -> String {
    let trimmed = value.trim_matches('"').trim_matches('\'');
    let mut output = String::with_capacity(trimmed.len());
    let mut remaining = trimmed;

    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find('}') else {
            output.push_str(&remaining[start..]);
            return output;
        };
        output.push_str(&expand_compose_variable(&after_start[..end]));
        remaining = &after_start[end + 1..];
    }

    output.push_str(remaining);
    output
}

fn expand_compose_variable(expression: &str) -> String {
    if let Some((name, default)) = expression.split_once(":-") {
        return match env::var(name) {
            Ok(value) if !value.is_empty() => value,
            _ => substitute_compose_env(default),
        };
    }
    if let Some((name, default)) = expression.split_once('-') {
        return match env::var(name) {
            Ok(value) => value,
            Err(_) => substitute_compose_env(default),
        };
    }

    env::var(expression).unwrap_or_default()
}

fn sanitize_project_name(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| character.to_lowercase())
        .filter(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        compose_name_from_file, compose_project_name, inspect_service_definition,
        sanitize_project_name, uses_compose_config,
    };
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

    #[test]
    fn compose_project_name_defaults_to_workspace_devcontainer() {
        let root = unique_temp_dir();
        let compose_file = root.join(".devcontainer").join("docker-compose.yml");
        fs::create_dir_all(compose_file.parent().expect("compose dir")).expect("compose dir");
        fs::write(&compose_file, "services:\n  app:\n    image: alpine:3.20\n").expect("compose");

        let project_name = compose_project_name(&[compose_file]).expect("project name");

        assert_eq!(
            project_name,
            root.file_name().unwrap().to_string_lossy().to_lowercase() + "_devcontainer"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compose_name_from_file_reads_top_level_name() {
        let root = unique_temp_dir();
        let compose_file = root.join("docker-compose.yml");
        fs::create_dir_all(&root).expect("compose dir");
        fs::write(
            &compose_file,
            "name: Custom-Project-Name\nservices:\n  app:\n    image: alpine:3.20\n",
        )
        .expect("compose");

        let project_name = compose_name_from_file(&compose_file)
            .expect("compose name")
            .expect("top-level name");

        assert_eq!(project_name, "Custom-Project-Name");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compose_name_from_file_supports_colon_dash_default_interpolation() {
        let root = unique_temp_dir();
        let compose_file = root.join("docker-compose.yml");
        let variable = format!("DEVCONTAINER_COMPOSE_TEST_MISSING_{}_A", std::process::id());
        fs::create_dir_all(&root).expect("compose dir");
        fs::write(
            &compose_file,
            format!("name: ${{{variable}:-MyProj}}\nservices:\n  app:\n    image: alpine:3.20\n"),
        )
        .expect("compose");

        let project_name = compose_name_from_file(&compose_file)
            .expect("compose name")
            .expect("top-level name");

        assert_eq!(project_name, "MyProj");
        assert_eq!(sanitize_project_name(&project_name), "myproj");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compose_name_from_file_supports_dash_default_interpolation() {
        let root = unique_temp_dir();
        let compose_file = root.join("docker-compose.yml");
        let variable = format!("DEVCONTAINER_COMPOSE_TEST_MISSING_{}_B", std::process::id());
        fs::create_dir_all(&root).expect("compose dir");
        fs::write(
            &compose_file,
            format!("name: ${{{variable}-MyProj}}\nservices:\n  app:\n    image: alpine:3.20\n"),
        )
        .expect("compose");

        let project_name = compose_name_from_file(&compose_file)
            .expect("compose name")
            .expect("top-level name");

        assert_eq!(project_name, "MyProj");
        assert_eq!(sanitize_project_name(&project_name), "myproj");
        let _ = fs::remove_dir_all(root);
    }
}
