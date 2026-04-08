use std::path::PathBuf;

use crate::commands::common;

use super::ComposeSpec;

pub(super) fn compose_args(spec: &ComposeSpec, subcommand: &str, tail: &[&str]) -> Vec<String> {
    compose_args_with_override(spec, subcommand, tail, None)
}

pub(super) fn compose_args_with_override(
    spec: &ComposeSpec,
    subcommand: &str,
    tail: &[&str],
    override_file: Option<&PathBuf>,
) -> Vec<String> {
    compose_args_owned(
        spec,
        subcommand,
        override_file,
        tail.iter().map(|value| value.to_string()).collect(),
    )
}

pub(super) fn compose_args_owned(
    spec: &ComposeSpec,
    subcommand: &str,
    override_file: Option<&PathBuf>,
    tail: Vec<String>,
) -> Vec<String> {
    let mut args = Vec::new();
    args.push("--project-name".to_string());
    args.push(spec.project_name.clone());
    for file in &spec.files {
        args.push("-f".to_string());
        args.push(file.display().to_string());
    }
    if let Some(override_file) = override_file {
        args.push("-f".to_string());
        args.push(override_file.display().to_string());
    }
    args.push(subcommand.to_string());
    args.extend(tail);
    args
}

pub(super) fn reject_unsupported_build_options(args: &[String]) -> Result<(), String> {
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
