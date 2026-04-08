use std::io::IsTerminal;

use serde_json::Value;

use super::context::{combined_remote_env, configured_user};

pub(crate) fn exec_command_and_args(args: &[String]) -> Result<Vec<String>, String> {
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if matches!(
            arg.as_str(),
            "--docker-path"
                | "--docker-compose-path"
                | "--workspace-folder"
                | "--config"
                | "--remote-env"
                | "--container-id"
                | "--id-label"
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

    Ok(args[index..].to_vec())
}

pub(crate) fn exec_engine_args(
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    container_id: &str,
    command_args: Vec<String>,
    interactive: bool,
) -> Vec<String> {
    let mut engine_args = vec!["exec".to_string()];
    if interactive {
        engine_args.push("-i".to_string());
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            engine_args.push("-t".to_string());
        }
    }
    engine_args.push("--workdir".to_string());
    engine_args.push(remote_workspace_folder.to_string());
    if let Some(user) = configured_user(configuration) {
        engine_args.push("--user".to_string());
        engine_args.push(user.to_string());
    }
    for (key, value) in combined_remote_env(args, Some(configuration)) {
        engine_args.push("-e".to_string());
        engine_args.push(format!("{key}={value}"));
    }
    engine_args.push(container_id.to_string());
    engine_args.extend(command_args);
    engine_args
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{exec_command_and_args, exec_engine_args};

    #[test]
    fn exec_command_and_args_rejects_unknown_options() {
        let error = exec_command_and_args(&[
            "--workspace-folder".to_string(),
            "/tmp/workspace".to_string(),
            "--mystery".to_string(),
        ])
        .expect_err("expected unsupported option");

        assert_eq!(error, "Unsupported exec option: --mystery");
    }

    #[test]
    fn exec_engine_args_include_workdir_user_and_remote_env() {
        let args = exec_engine_args(
            &["--remote-env".to_string(), "CLI_ENV=cli".to_string()],
            &json!({
                "remoteUser": "vscode",
                "remoteEnv": {
                    "CONFIG_ENV": "config"
                }
            }),
            "/workspace",
            "container-id",
            vec!["/bin/echo".to_string(), "hello".to_string()],
            false,
        );

        assert_eq!(args[0], "exec");
        assert!(args.contains(&"--workdir".to_string()));
        assert!(args.contains(&"/workspace".to_string()));
        assert!(args.contains(&"--user".to_string()));
        assert!(args.contains(&"vscode".to_string()));
        assert!(args.contains(&"container-id".to_string()));
        assert!(args.contains(&"/bin/echo".to_string()));
        assert!(args.iter().any(|arg| arg == "CONFIG_ENV=config"));
        assert!(args.iter().any(|arg| arg == "CLI_ENV=cli"));
    }

    #[test]
    fn exec_command_and_args_accepts_docker_compose_path() {
        let args = exec_command_and_args(&[
            "--docker-compose-path".to_string(),
            "/usr/local/bin/podman-compose".to_string(),
            "/bin/echo".to_string(),
            "hello".to_string(),
        ])
        .expect("command args");

        assert_eq!(args, vec!["/bin/echo".to_string(), "hello".to_string()]);
    }
}
