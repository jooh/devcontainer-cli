//! Dotfiles installation command construction for lifecycle execution.

use crate::commands::common;

pub(super) fn dotfiles_install_command(args: &[String]) -> Option<String> {
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
