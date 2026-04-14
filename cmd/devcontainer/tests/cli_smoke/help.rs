//! CLI smoke tests for help text and unsupported command surfaces.

use crate::support::test_support::devcontainer_command;

#[test]
fn top_level_help_matches_public_cli_surface() {
    let output = devcontainer_command(None)
        .arg("--help")
        .output()
        .expect("help command should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("devcontainer <command>"), "{stdout}");
    assert!(stdout.contains("devcontainer up"), "{stdout}");
    assert!(
        stdout.contains("devcontainer exec <cmd> [args..]"),
        "{stdout}"
    );
    assert!(stdout.contains("Options:"), "{stdout}");
    assert!(stdout.contains("--version"), "{stdout}");
    assert!(
        !stdout.contains("docs/cli/command-reference.md"),
        "{stdout}"
    );
    assert!(!stdout.contains("parity-inventory"), "{stdout}");
    assert!(!stdout.contains("Current state:"), "{stdout}");
}

#[test]
fn up_help_lists_upstream_options() {
    let output = devcontainer_command(None)
        .args(["up", "--help"])
        .output()
        .expect("up help should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("devcontainer up"), "{stdout}");
    assert!(stdout.contains("--workspace-folder"), "{stdout}");
    assert!(stdout.contains("--build-no-cache"), "{stdout}");
    assert!(stdout.contains("--dotfiles-target-path"), "{stdout}");
    assert!(
        stdout.contains("--include-merged-configuration"),
        "{stdout}"
    );
    assert!(!stdout.contains("Current state:"), "{stdout}");
}

#[test]
fn features_help_matches_upstream_group_shape() {
    let output = devcontainer_command(None)
        .args(["features", "--help"])
        .output()
        .expect("features help should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("devcontainer features"), "{stdout}");
    assert!(
        stdout.contains("devcontainer features test [target]"),
        "{stdout}"
    );
    assert!(
        stdout.contains("devcontainer features package <target>"),
        "{stdout}"
    );
    assert!(
        stdout.contains("devcontainer features publish <target>"),
        "{stdout}"
    );
    assert!(!stdout.contains("list"), "{stdout}");
    assert!(!stdout.contains("Native support:"), "{stdout}");
}

#[test]
fn templates_apply_help_lists_nested_options() {
    let output = devcontainer_command(None)
        .args(["templates", "apply", "--help"])
        .output()
        .expect("templates apply help should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("devcontainer templates apply"), "{stdout}");
    assert!(stdout.contains("--template-id"), "{stdout}");
    assert!(stdout.contains("--omit-paths"), "{stdout}");
    assert!(stdout.contains("--workspace-folder"), "{stdout}");
}

#[test]
fn native_only_collection_list_commands_are_not_supported() {
    for args in [
        ["features", "list"].as_slice(),
        ["features", "ls"].as_slice(),
        ["templates", "list"].as_slice(),
        ["templates", "ls"].as_slice(),
    ] {
        let output = devcontainer_command(None)
            .args(args)
            .output()
            .expect("collection list command should run");

        assert!(!output.status.success(), "{output:?}");
    }
}

#[test]
fn only_top_level_long_version_flag_is_supported() {
    let output = devcontainer_command(None)
        .arg("--version")
        .output()
        .expect("version command should run");

    assert!(output.status.success(), "{output:?}");

    for args in [["-V"], ["version"]] {
        let output = devcontainer_command(None)
            .args(args)
            .output()
            .expect("version alias should run");

        assert!(!output.status.success(), "{output:?}");
    }
}

#[test]
fn unsupported_visible_command_option_fails_with_native_message() {
    let output = devcontainer_command(None)
        .args(["outdated", "--log-level", "trace"])
        .output()
        .expect("outdated command should run");

    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("--log-level"), "{stderr}");
    assert!(
        stderr.contains("not yet implemented in the native Rust CLI"),
        "{stderr}"
    );
    assert!(stderr.contains("devcontainer outdated"), "{stderr}");
}

#[test]
fn help_omits_hidden_upstream_options() {
    let output = devcontainer_command(None)
        .args(["up", "--help"])
        .output()
        .expect("up help should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("--omit-syntax-directive"), "{stdout}");
}

#[test]
fn help_marks_unsupported_options_inline() {
    let output = devcontainer_command(None)
        .args(["outdated", "--help"])
        .output()
        .expect("outdated help should run");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let marked_line = stdout
        .lines()
        .find(|line| line.contains("--log-level"))
        .expect("marked unsupported option");
    assert!(
        marked_line.contains("[not yet implemented in native Rust CLI]"),
        "{stdout}"
    );
}
