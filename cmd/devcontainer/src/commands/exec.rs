use std::collections::HashMap;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use super::common;
use crate::process_runner::{self, ProcessRequest, ProcessResult};

pub(crate) fn run(args: &[String]) -> ExitCode {
    if common::has_flag(args, "--interactive") {
        return match stream_native_exec(args) {
            Ok(status_code) => ExitCode::from(status_code as u8),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        };
    }

    match execute_native_exec(args) {
        Ok(result) => {
            let _ = io::stdout().write_all(result.stdout.as_bytes());
            let _ = io::stderr().write_all(result.stderr.as_bytes());
            ExitCode::from(result.status_code as u8)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn exec_command_and_args(args: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    let workspace_folder = common::parse_option_value(args, "--workspace-folder")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| "Unable to determine workspace folder".to_string())?;

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if matches!(
            arg.as_str(),
            "--workspace-folder" | "--config" | "--remote-env"
        ) {
            index += 2;
            continue;
        }
        if arg == "--interactive" {
            index += 1;
            continue;
        }
        if arg.starts_with("--") {
            return Err(format!("Unsupported exec option: {arg}"));
        }
        break;
    }

    if index >= args.len() {
        return Err("exec requires a command to run".to_string());
    }

    Ok((workspace_folder, args[index..].to_vec()))
}

fn build_exec_env(args: &[String]) -> HashMap<String, String> {
    let mut remote_env = env::vars().collect::<HashMap<_, _>>();
    remote_env.extend(common::remote_env_overrides(args));
    remote_env
}

pub(crate) fn execute_native_exec(args: &[String]) -> Result<ProcessResult, String> {
    let (workspace_folder, command_args) = exec_command_and_args(args)?;

    process_runner::run_process(&ProcessRequest {
        program: command_args[0].clone(),
        args: command_args[1..].to_vec(),
        cwd: Some(workspace_folder),
        env: build_exec_env(args),
    })
}

fn stream_native_exec(args: &[String]) -> Result<i32, String> {
    let (workspace_folder, command_args) = exec_command_and_args(args)?;

    process_runner::run_process_streaming(&ProcessRequest {
        program: command_args[0].clone(),
        args: command_args[1..].to_vec(),
        cwd: Some(workspace_folder),
        env: build_exec_env(args),
    })
}

#[cfg(test)]
mod tests {
    use super::execute_native_exec;

    #[test]
    fn execute_native_exec_runs_non_interactive_command() {
        let result = execute_native_exec(&[
            "/bin/sh".to_string(),
            "-c".to_string(),
            "printf native-exec".to_string(),
        ])
        .expect("exec result");

        assert_eq!(result.status_code, 0);
        assert_eq!(result.stdout, "native-exec");
        assert_eq!(result.stderr, "");
    }
}
