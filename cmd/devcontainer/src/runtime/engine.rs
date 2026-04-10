use crate::commands::common;
use crate::process_runner::{self, ProcessRequest, ProcessResult};

pub(crate) fn engine_request(args: &[String], engine_args: Vec<String>) -> ProcessRequest {
    common::runtime_process_request(args, engine_program(args), engine_args, None)
}

pub(crate) fn run_engine(
    args: &[String],
    engine_args: Vec<String>,
) -> Result<ProcessResult, String> {
    process_runner::run_process(&engine_request(args, engine_args))
}

pub(crate) fn run_engine_streaming(
    args: &[String],
    engine_args: Vec<String>,
) -> Result<i32, String> {
    process_runner::run_process_streaming(&engine_request(args, engine_args))
}

pub(crate) fn compose_request(args: &[String], compose_args: Vec<String>) -> ProcessRequest {
    if let Some(compose_program) = common::parse_option_value(args, "--docker-compose-path") {
        common::runtime_process_request(args, compose_program, compose_args, None)
    } else {
        let mut args_with_subcommand = vec!["compose".to_string()];
        args_with_subcommand.extend(compose_args);
        common::runtime_process_request(args, engine_program(args), args_with_subcommand, None)
    }
}

pub(crate) fn run_compose(
    args: &[String],
    compose_args: Vec<String>,
) -> Result<ProcessResult, String> {
    process_runner::run_process(&compose_request(args, compose_args))
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

#[cfg(test)]
mod tests {
    use crate::process_runner::ProcessLogLevel;

    use super::engine_request;

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
}
