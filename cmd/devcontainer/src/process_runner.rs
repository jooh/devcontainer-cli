use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub struct ProcessRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
}

pub struct ProcessResult {
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_process(request: &ProcessRequest) -> Result<ProcessResult, String> {
    let mut command = Command::new(&request.program);
    command.args(&request.args);

    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd);
    }

    if !request.env.is_empty() {
        command.envs(&request.env);
    }

    let output = command.output().map_err(|error| error.to_string())?;
    Ok(ProcessResult {
        status_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn run_process_streaming(request: &ProcessRequest) -> Result<i32, String> {
    let mut command = Command::new(&request.program);
    command.args(&request.args);

    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd);
    }

    if !request.env.is_empty() {
        command.envs(&request.env);
    }

    let status = command.status().map_err(|error| error.to_string())?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::{run_process, run_process_streaming, ProcessRequest};
    use std::collections::HashMap;

    #[test]
    fn captures_stdout_and_exit_status() {
        let result = run_process(&ProcessRequest {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "printf native-process".to_string()],
            cwd: None,
            env: HashMap::new(),
        })
        .expect("expected process to run");

        assert_eq!(result.status_code, 0);
        assert_eq!(result.stdout, "native-process");
        assert_eq!(result.stderr, "");
    }

    #[test]
    fn returns_status_for_streaming_processes() {
        let status = run_process_streaming(&ProcessRequest {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exit 0".to_string()],
            cwd: None,
            env: HashMap::new(),
        })
        .expect("expected streaming process to run");

        assert_eq!(status, 0);
    }
}
