use std::path::Path;
use std::thread;

use serde_json::Value;

use crate::commands::common;
use crate::process_runner::{self, ProcessRequest};

use super::context::{combined_remote_env, configured_user};
use super::engine;

#[derive(Clone, Copy)]
pub(crate) enum LifecycleMode {
    UpCreated,
    UpStarted,
    UpReused,
    SetUp,
    RunUserCommands,
}

enum LifecycleCommand {
    Shell(String),
    Exec(Vec<String>),
}

enum LifecycleStep {
    CommandGroup(Vec<LifecycleCommand>),
    InstallDotfiles,
}

pub(crate) fn run_lifecycle_commands(
    container_id: &str,
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    mode: LifecycleMode,
) -> Result<(), String> {
    for step in selected_lifecycle_steps(configuration, args, mode) {
        match step {
            LifecycleStep::CommandGroup(command_group) => {
                run_process_group(command_group, |command| {
                    lifecycle_exec_args(
                        args,
                        configuration,
                        remote_workspace_folder,
                        container_id,
                        command,
                    )
                    .map(|engine_args| engine::engine_request(args, engine_args))
                })?;
            }
            LifecycleStep::InstallDotfiles => {
                let Some(command) = dotfiles_install_command(args) else {
                    continue;
                };
                run_process_group(vec![LifecycleCommand::Shell(command)], |command| {
                    lifecycle_exec_args(
                        args,
                        configuration,
                        remote_workspace_folder,
                        container_id,
                        command,
                    )
                    .map(|engine_args| engine::engine_request(args, engine_args))
                })?;
            }
        }
    }

    Ok(())
}

pub(crate) fn run_initialize_command(
    args: &[String],
    configuration: &Value,
    workspace_folder: &Path,
) -> Result<(), String> {
    let Some(command_group) = lifecycle_command_value(configuration, "initializeCommand") else {
        return Ok(());
    };

    run_process_group(command_group, |command| {
        Ok(host_lifecycle_request(args, workspace_folder, command))
    })
}

fn run_process_group(
    command_group: Vec<LifecycleCommand>,
    build_request: impl Fn(LifecycleCommand) -> Result<ProcessRequest, String>,
) -> Result<(), String> {
    if command_group.len() == 1 {
        let result = process_runner::run_process(&build_request(
            command_group
                .into_iter()
                .next()
                .expect("single lifecycle command"),
        )?)?;
        if result.status_code != 0 {
            return Err(engine::stderr_or_stdout(&result));
        }
        return Ok(());
    }

    let handles = command_group
        .into_iter()
        .map(|command| {
            let request = build_request(command);
            thread::spawn(move || match request {
                Ok(request) => process_runner::run_process(&request),
                Err(error) => Err(error),
            })
        })
        .collect::<Vec<_>>();

    let mut first_error = None;
    for handle in handles {
        match handle.join() {
            Ok(Ok(result)) if result.status_code == 0 => {}
            Ok(Ok(result)) => {
                if first_error.is_none() {
                    first_error = Some(engine::stderr_or_stdout(&result));
                }
            }
            Ok(Err(error)) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            Err(_) => {
                if first_error.is_none() {
                    first_error =
                        Some("Lifecycle command thread panicked unexpectedly".to_string());
                }
            }
        }
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(())
}

fn selected_lifecycle_steps(
    configuration: &Value,
    args: &[String],
    mode: LifecycleMode,
) -> Vec<LifecycleStep> {
    let skip_post_create = common::has_flag(args, "--skip-post-create");
    let skip_post_attach = common::has_flag(args, "--skip-post-attach");
    let skip_non_blocking = common::has_flag(args, "--skip-non-blocking-commands");

    if skip_post_create {
        return Vec::new();
    }

    let wait_for = configuration
        .get("waitFor")
        .and_then(Value::as_str)
        .unwrap_or("updateContentCommand");
    if skip_non_blocking && wait_for == "initializeCommand" {
        return Vec::new();
    }

    let mut steps = Vec::new();
    let lifecycle_stages = [
        (
            "onCreateCommand",
            lifecycle_command_value(configuration, "onCreateCommand"),
        ),
        (
            "updateContentCommand",
            lifecycle_command_value(configuration, "updateContentCommand"),
        ),
        (
            "postCreateCommand",
            lifecycle_command_value(configuration, "postCreateCommand"),
        ),
        (
            "postStartCommand",
            lifecycle_command_value(configuration, "postStartCommand"),
        ),
        (
            "postAttachCommand",
            (!skip_post_attach)
                .then(|| lifecycle_command_value(configuration, "postAttachCommand"))
                .flatten(),
        ),
    ];

    for (stage, command_group) in lifecycle_stages {
        if lifecycle_stage_runs_in_mode(stage, mode) {
            if let Some(command_group) = command_group {
                steps.push(LifecycleStep::CommandGroup(command_group));
            }
        }
        if skip_non_blocking && stage == wait_for {
            break;
        }
        if stage == "postCreateCommand"
            && lifecycle_stage_runs_in_mode(stage, mode)
            && dotfiles_install_command(args).is_some()
        {
            steps.push(LifecycleStep::InstallDotfiles);
        }
    }

    steps
}

fn lifecycle_stage_runs_in_mode(stage: &str, mode: LifecycleMode) -> bool {
    match mode {
        LifecycleMode::UpCreated | LifecycleMode::SetUp | LifecycleMode::RunUserCommands => true,
        LifecycleMode::UpStarted => matches!(stage, "postStartCommand" | "postAttachCommand"),
        LifecycleMode::UpReused => stage == "postAttachCommand",
    }
}

fn lifecycle_command_value(configuration: &Value, key: &str) -> Option<Vec<LifecycleCommand>> {
    let value = configuration.get(key)?;
    lifecycle_command_group(value)
}

fn lifecycle_command_group(value: &Value) -> Option<Vec<LifecycleCommand>> {
    match value {
        Value::String(command) => Some(vec![LifecycleCommand::Shell(command.clone())]),
        Value::Array(parts) => {
            command_parts(parts).map(|parts| vec![LifecycleCommand::Exec(parts)])
        }
        Value::Object(entries) => {
            let commands = entries
                .values()
                .filter_map(lifecycle_command)
                .collect::<Vec<_>>();
            if commands.is_empty() {
                None
            } else {
                Some(commands)
            }
        }
        _ => None,
    }
}

fn lifecycle_command(value: &Value) -> Option<LifecycleCommand> {
    match value {
        Value::String(command) => Some(LifecycleCommand::Shell(command.clone())),
        Value::Array(parts) => command_parts(parts).map(LifecycleCommand::Exec),
        _ => None,
    }
}

fn command_parts(parts: &[Value]) -> Option<Vec<String>> {
    let parts = parts
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn lifecycle_exec_args(
    args: &[String],
    configuration: &Value,
    remote_workspace_folder: &str,
    container_id: &str,
    command: LifecycleCommand,
) -> Result<Vec<String>, String> {
    let mut engine_args = vec![
        "exec".to_string(),
        "--workdir".to_string(),
        remote_workspace_folder.to_string(),
    ];
    if let Some(user) = configured_user(configuration) {
        engine_args.push("--user".to_string());
        engine_args.push(user.to_string());
    }
    for (key, value) in combined_remote_env(args, Some(configuration))? {
        engine_args.push("-e".to_string());
        engine_args.push(format!("{key}={value}"));
    }
    engine_args.push(container_id.to_string());
    match command {
        LifecycleCommand::Shell(command) => {
            engine_args.push("/bin/sh".to_string());
            engine_args.push("-lc".to_string());
            engine_args.push(command);
        }
        LifecycleCommand::Exec(parts) => engine_args.extend(parts),
    }
    Ok(engine_args)
}

fn dotfiles_install_command(args: &[String]) -> Option<String> {
    let options = common::runtime_options(args);
    let repository = normalize_dotfiles_repository(options.dotfiles_repository.as_deref()?);
    let target_path = options
        .dotfiles_target_path
        .unwrap_or_else(|| "~/dotfiles".to_string());
    let marker_file = format!(
        "{}/.dotfilesMarker",
        options
            .container_data_folder
            .unwrap_or_else(|| "~/.devcontainer".to_string())
            .trim_end_matches('/')
    );

    let mut script = vec![
        format!(
            "{} || (echo dotfiles marker found && exit 1) || exit 0",
            create_file_command(&marker_file)
        ),
        "command -v git >/dev/null 2>&1 || (echo git not found && exit 1) || exit 0".to_string(),
        format!(
            "[ -e {} ] || git clone --depth 1 {} {} || exit $?",
            shell_path_argument(&target_path),
            shell_single_quote(&repository),
            shell_path_argument(&target_path)
        ),
        format!("echo Setting current directory to {}", target_path),
        format!("cd {}", shell_path_argument(&target_path)),
    ];

    if let Some(install_command) = options.dotfiles_install_command {
        script.extend(dotfiles_explicit_install_commands(&install_command));
    } else {
        script.extend(dotfiles_default_install_commands());
    }

    Some(script.join("\n"))
}

fn normalize_dotfiles_repository(repository: &str) -> String {
    if repository.contains(':')
        || repository.starts_with("./")
        || repository.starts_with("../")
        || repository.starts_with('/')
    {
        repository.to_string()
    } else {
        format!("https://github.com/{repository}.git")
    }
}

fn create_file_command(location: &str) -> String {
    format!(
        "test ! -f {location} && set -o noclobber && mkdir -p {parent} && {{ > {location} ; }} 2> /dev/null",
        location = shell_path_argument(location),
        parent = shell_path_argument(shell_parent(location))
    )
}

fn shell_parent(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or(".")
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_path_argument(value: &str) -> String {
    if value.starts_with("~/") {
        value.to_string()
    } else {
        shell_single_quote(value)
    }
}

fn dotfiles_explicit_install_commands(install_command: &str) -> Vec<String> {
    let quoted = shell_single_quote(install_command);
    let dotted = shell_single_quote(&format!("./{install_command}"));
    vec![
        format!("if [ -f {dotted} ]", dotted = dotted),
        "then".to_string(),
        format!("  install_path={dotted}", dotted = dotted),
        format!("elif [ -f {quoted} ]", quoted = quoted),
        "then".to_string(),
        format!("  install_path={quoted}", quoted = quoted),
        "else".to_string(),
        format!("  echo Could not locate {quoted}", quoted = quoted),
        "  exit 126".to_string(),
        "fi".to_string(),
        "if [ ! -x \"$install_path\" ]".to_string(),
        "then".to_string(),
        "  chmod +x \"$install_path\"".to_string(),
        "fi".to_string(),
        "echo Executing command \"$install_path\"...".to_string(),
        "\"$install_path\"".to_string(),
    ]
}

fn dotfiles_default_install_commands() -> Vec<String> {
    vec![
        "install_path=''".to_string(),
        "for f in install.sh install bootstrap.sh bootstrap script/bootstrap setup.sh setup script/setup".to_string(),
        "do".to_string(),
        "  if [ -e \"$f\" ]".to_string(),
        "  then".to_string(),
        "    install_path=\"$f\"".to_string(),
        "    break".to_string(),
        "  fi".to_string(),
        "done".to_string(),
        "if [ -z \"$install_path\" ]".to_string(),
        "then".to_string(),
        "  dotfiles=$(find \"$(pwd)\" -mindepth 1 -maxdepth 1 -name '.*' ! -name '.git' -print)".to_string(),
        "  if [ ! -z \"$dotfiles\" ]".to_string(),
        "  then".to_string(),
        "    echo Linking dotfiles: $dotfiles".to_string(),
        "    ln -sf $dotfiles ~ 2>/dev/null".to_string(),
        "  else".to_string(),
        "    echo No dotfiles found.".to_string(),
        "  fi".to_string(),
        "else".to_string(),
        "  if [ ! -x \"$install_path\" ]".to_string(),
        "  then".to_string(),
        "    chmod +x \"$install_path\"".to_string(),
        "  fi".to_string(),
        "  echo Executing command \"$install_path\"...".to_string(),
        "  ./\"$install_path\"".to_string(),
        "fi".to_string(),
    ]
}

fn host_lifecycle_request(
    args: &[String],
    workspace_folder: &Path,
    command: LifecycleCommand,
) -> ProcessRequest {
    match command {
        LifecycleCommand::Shell(command) => common::runtime_process_request(
            args,
            "/bin/sh".to_string(),
            vec!["-c".to_string(), command],
            Some(workspace_folder.to_path_buf()),
        ),
        LifecycleCommand::Exec(parts) => {
            let mut parts = parts.into_iter();
            common::runtime_process_request(
                args,
                parts.next().unwrap_or_default(),
                parts.collect(),
                Some(workspace_folder.to_path_buf()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        dotfiles_install_command, lifecycle_command_group, lifecycle_exec_args,
        selected_lifecycle_steps, LifecycleCommand, LifecycleMode, LifecycleStep,
    };

    #[test]
    fn lifecycle_command_group_supports_strings_arrays_and_objects() {
        assert!(lifecycle_command_group(&json!("echo hello")).is_some());
        assert!(lifecycle_command_group(&json!(["/bin/echo", "hello"])).is_some());
        assert!(lifecycle_command_group(&json!({
            "a": "echo one",
            "b": ["/bin/echo", "two"]
        }))
        .is_some());
    }

    #[test]
    fn selected_lifecycle_steps_respect_mode_and_wait_for() {
        let steps = selected_lifecycle_steps(
            &json!({
                "onCreateCommand": "echo on-create",
                "updateContentCommand": "echo update",
                "postCreateCommand": "echo post-create",
                "postStartCommand": "echo post-start",
                "postAttachCommand": "echo post-attach",
                "waitFor": "postStartCommand"
            }),
            &["--skip-non-blocking-commands".to_string()],
            LifecycleMode::RunUserCommands,
        );

        assert_eq!(steps.len(), 4);

        let reused = selected_lifecycle_steps(
            &json!({
                "postStartCommand": "echo post-start",
                "postAttachCommand": "echo post-attach"
            }),
            &[],
            LifecycleMode::UpReused,
        );

        assert_eq!(reused.len(), 1);
    }

    #[test]
    fn selected_lifecycle_steps_insert_dotfiles_after_post_create() {
        let steps = selected_lifecycle_steps(
            &json!({
                "postCreateCommand": "echo post-create",
                "postStartCommand": "echo post-start"
            }),
            &[
                "--dotfiles-repository".to_string(),
                "./dotfiles".to_string(),
            ],
            LifecycleMode::RunUserCommands,
        );

        assert!(matches!(steps[0], LifecycleStep::CommandGroup(_)));
        assert!(matches!(steps[1], LifecycleStep::InstallDotfiles));
        assert!(matches!(steps[2], LifecycleStep::CommandGroup(_)));
    }

    #[test]
    fn lifecycle_exec_args_use_absolute_shell_path() {
        let args = lifecycle_exec_args(
            &[],
            &json!({}),
            "/workspaces/sample",
            "container-id",
            LifecycleCommand::Shell("echo hello".to_string()),
        )
        .expect("lifecycle exec args");

        assert!(
            args.contains(&"/bin/sh".to_string()),
            "expected lifecycle shell command to use /bin/sh: {args:?}"
        );
    }

    #[test]
    fn dotfiles_install_command_defaults_target_path_and_marker_folder() {
        let command = dotfiles_install_command(&[
            "--dotfiles-repository".to_string(),
            "owner/repo".to_string(),
        ])
        .expect("dotfiles command");

        assert!(command.contains("https://github.com/owner/repo.git"));
        assert!(command.contains("~/.devcontainer/.dotfilesMarker"));
        assert!(command.contains("~/dotfiles"));
    }
}
