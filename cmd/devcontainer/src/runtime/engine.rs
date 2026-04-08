use std::collections::HashMap;

use crate::commands::common;
use crate::process_runner::{self, ProcessRequest, ProcessResult};

pub(crate) fn engine_request(args: &[String], engine_args: Vec<String>) -> ProcessRequest {
    ProcessRequest {
        program: engine_program(args),
        args: engine_args,
        cwd: None,
        env: HashMap::new(),
    }
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
        ProcessRequest {
            program: compose_program,
            args: compose_args,
            cwd: None,
            env: HashMap::new(),
        }
    } else {
        let mut args_with_subcommand = vec!["compose".to_string()];
        args_with_subcommand.extend(compose_args);
        ProcessRequest {
            program: engine_program(args),
            args: args_with_subcommand,
            cwd: None,
            env: HashMap::new(),
        }
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
