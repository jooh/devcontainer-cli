//! Container engine invocation helpers for native runtime commands.

use std::io;
use std::path::Path;

use crate::commands::common;
use crate::process_runner::{self, ProcessRequest, ProcessResult};

pub(crate) fn engine_request(args: &[String], engine_args: Vec<String>) -> ProcessRequest {
    let mut request =
        common::runtime_process_request(args, engine_program(args), engine_args, None);
    let request_args = request.args.clone();
    apply_buildkit_env(args, &request_args, &mut request);
    request
}

pub(crate) fn run_engine(
    args: &[String],
    engine_args: Vec<String>,
) -> Result<ProcessResult, String> {
    let request = engine_request(args, engine_args);
    process_runner::run_process(&request)
        .map_err(|error| normalize_process_error(args, &request, error))
}

pub(crate) fn run_engine_streaming(
    args: &[String],
    engine_args: Vec<String>,
) -> Result<i32, String> {
    let request = engine_request(args, engine_args);
    process_runner::run_process_streaming(&request)
        .map_err(|error| normalize_process_error(args, &request, error))
}

pub(crate) fn compose_request(args: &[String], compose_args: Vec<String>) -> ProcessRequest {
    if let Some(compose_program) = common::parse_option_value(args, "--docker-compose-path") {
        let mut request =
            common::runtime_process_request(args, compose_program, compose_args, None);
        let request_args = request.args.clone();
        apply_buildkit_env(args, &request_args, &mut request);
        request
    } else {
        let mut args_with_subcommand = vec!["compose".to_string()];
        args_with_subcommand.extend(compose_args);
        let mut request =
            common::runtime_process_request(args, engine_program(args), args_with_subcommand, None);
        let request_args = request.args.clone();
        apply_buildkit_env(args, &request_args, &mut request);
        request
    }
}

pub(crate) fn run_compose(
    args: &[String],
    compose_args: Vec<String>,
) -> Result<ProcessResult, String> {
    let request = compose_request(args, compose_args);
    process_runner::run_process(&request)
        .map_err(|error| normalize_process_error(args, &request, error))
}

pub(crate) fn stderr_or_stdout(result: &ProcessResult) -> String {
    if result.stderr.trim().is_empty() {
        result.stdout.trim().to_string()
    } else {
        result.stderr.trim().to_string()
    }
}

fn engine_program(args: &[String]) -> String {
    common::parse_option_value(args, "--docker-path").unwrap_or_else(|| "docker".to_string())
}

fn normalize_process_error(args: &[String], request: &ProcessRequest, error: io::Error) -> String {
    if error.kind() != io::ErrorKind::NotFound {
        return error.to_string();
    }

    let executable = request.program.as_str();
    if common::parse_option_value(args, "--docker-compose-path")
        .as_deref()
        .is_some_and(|program| program == executable)
    {
        return format!(
            "Container compose executable not found: {executable}. Verify --docker-compose-path or install the requested compose CLI."
        );
    }

    let requested_engine = common::parse_option_value(args, "--docker-path");
    if requested_engine.is_none()
        && Path::new(executable)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("docker"))
    {
        return "Container engine executable not found: docker. Install Docker or rerun with --docker-path podman.".to_string();
    }

    format!(
        "Container engine executable not found: {executable}. Verify --docker-path or install the requested container engine."
    )
}

fn apply_buildkit_env(args: &[String], request_args: &[String], request: &mut ProcessRequest) {
    if !is_build_request(request_args) {
        return;
    }
    match common::runtime_options(args).buildkit.as_deref() {
        Some("never") => {
            request
                .env
                .insert("DOCKER_BUILDKIT".to_string(), "0".to_string());
        }
        Some("auto") => {
            request
                .env
                .insert("DOCKER_BUILDKIT".to_string(), "1".to_string());
        }
        _ => {}
    }
}

fn is_build_request(request_args: &[String]) -> bool {
    let mut index = usize::from(request_args.first().map(String::as_str) == Some("compose"));

    if request_args.get(index).map(String::as_str) == Some("build") {
        return true;
    }

    while index < request_args.len() {
        match request_args[index].as_str() {
            "--project-name" | "-f" => {
                index += 2;
            }
            value if value.starts_with('-') => {
                index += 1;
            }
            "build" => return true,
            _ => return false,
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use crate::process_runner::ProcessLogLevel;

    use super::{compose_request, engine_request, is_build_request};

    #[test]
    fn engine_request_applies_terminal_env_and_log_level() {
        let request = engine_request(
            &[
                "--log-level".to_string(),
                "debug".to_string(),
                "--terminal-columns".to_string(),
                "160".to_string(),
                "--terminal-rows".to_string(),
                "48".to_string(),
            ],
            vec!["ps".to_string()],
        );

        assert_eq!(request.log_level, ProcessLogLevel::Debug);
        assert_eq!(request.env.get("COLUMNS").map(String::as_str), Some("160"));
        assert_eq!(request.env.get("LINES").map(String::as_str), Some("48"));
    }

    #[test]
    fn detects_build_requests_for_compose_invocations() {
        assert!(is_build_request(&["build".to_string()]));
        assert!(is_build_request(&[
            "compose".to_string(),
            "build".to_string(),
            "app".to_string(),
        ]));
        assert!(is_build_request(&[
            "--project-name".to_string(),
            "workspace".to_string(),
            "-f".to_string(),
            "docker-compose.yml".to_string(),
            "build".to_string(),
            "app".to_string(),
        ]));
        assert!(is_build_request(&[
            "compose".to_string(),
            "--project-name".to_string(),
            "workspace".to_string(),
            "build".to_string(),
            "app".to_string(),
        ]));
        assert!(!is_build_request(&[
            "--project-name".to_string(),
            "workspace".to_string(),
            "up".to_string(),
        ]));
        assert!(!is_build_request(&[
            "compose".to_string(),
            "up".to_string(),
        ]));
    }

    #[test]
    fn compose_request_applies_buildkit_env_for_default_docker_compose_builds() {
        let request = compose_request(
            &["--buildkit".to_string(), "never".to_string()],
            vec!["build".to_string(), "app".to_string()],
        );

        assert_eq!(
            request.env.get("DOCKER_BUILDKIT").map(String::as_str),
            Some("0")
        );
    }
}
