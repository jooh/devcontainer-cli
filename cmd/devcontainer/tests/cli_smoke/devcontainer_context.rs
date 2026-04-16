//! Smoke tests for the local devcontainer context helper script.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::support::test_support::{repo_root, unique_temp_dir};

fn write_executable_script(path: &Path, body: &str) {
    fs::write(path, body).expect("script");
    let mut permissions = fs::metadata(path).expect("script metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
    }
    fs::set_permissions(path, permissions).expect("script permissions");
}

#[test]
fn reset_uses_resolved_compose_project_name() {
    let root = unique_temp_dir("devcontainer-cli-smoke");
    let workspace = root.join("workspace");
    let home = root.join("home");
    let bin = root.join("bin");
    let log_dir = root.join("logs");
    fs::create_dir_all(workspace.join(".devcontainer")).expect("workspace config dir");
    fs::create_dir_all(&home).expect("home dir");
    fs::create_dir_all(&bin).expect("bin dir");
    fs::create_dir_all(&log_dir).expect("log dir");
    fs::write(
        workspace.join(".devcontainer").join("devcontainer.json"),
        "{\n  \"dockerComposeFile\": \"docker-compose.yml\",\n  \"service\": \"app\",\n  \"workspaceFolder\": \"/workspace\"\n}\n",
    )
    .expect("config");
    fs::write(
        workspace.join(".devcontainer").join("docker-compose.yml"),
        "name: Custom-Project-Name\nservices:\n  app:\n    image: alpine:3.20\n",
    )
    .expect("compose");

    let devcontainer_log = log_dir.join("devcontainer.log");
    let podman_log = log_dir.join("podman.log");
    write_executable_script(
        &bin.join("devcontainer"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nexit 0\n",
            devcontainer_log.display()
        ),
    );
    write_executable_script(
        &bin.join("podman"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nexit 0\n",
            podman_log.display()
        ),
    );
    write_executable_script(&bin.join("podman-compose"), "#!/bin/sh\nexit 0\n");

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .arg(repo_root().join("devcontainer-context.sh"))
        .arg("--reset")
        .current_dir(&workspace)
        .env("HOME", &home)
        .env("PATH", path)
        .output()
        .expect("script should run");

    assert!(output.status.success(), "{output:?}");
    let podman_log = fs::read_to_string(&podman_log).expect("podman log");
    assert!(
        podman_log.contains("--project-name custom-project-name down -v --remove-orphans"),
        "{podman_log}"
    );

    let _ = fs::remove_dir_all(root);
}
