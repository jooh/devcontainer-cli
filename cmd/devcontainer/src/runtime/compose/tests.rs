use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::override_file::compose_metadata_override_file;
use super::project::{
    compose_name_from_file, compose_project_name, sanitize_project_name, substitute_compose_env,
};
use super::service::{
    compose_image_name_separator, inspect_service_definition, parse_semver_prefix,
};
use super::uses_compose_config;

static NEXT_TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let unique_id = NEXT_TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "devcontainer-compose-test-{}-{suffix}-{unique_id}",
        std::process::id()
    ))
}

fn init_git_repo(root: &std::path::Path) {
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .status()
        .expect("git init");
    assert!(status.success(), "git init failed: {status:?}");
}

#[test]
fn detects_compose_configs() {
    assert!(uses_compose_config(&json!({
        "dockerComposeFile": "docker-compose.yml",
        "service": "app"
    })));
    assert!(!uses_compose_config(&json!({
        "image": "alpine:3.20"
    })));
}

#[test]
fn inspects_service_image_and_build_presence() {
    let root = unique_temp_dir();
    let compose_file = root.join("docker-compose.yml");
    fs::create_dir_all(&root).expect("compose root");
    fs::write(
        &compose_file,
        "services:\n  app:\n    image: example/native-compose:test\n    build:\n      context: .\n",
    )
    .expect("compose file");

    let (image, has_build) =
        inspect_service_definition(&[compose_file], "app").expect("service definition");

    assert_eq!(image.as_deref(), Some("example/native-compose:test"));
    assert!(has_build);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compose_project_name_defaults_to_workspace_devcontainer() {
    let root = unique_temp_dir();
    let compose_file = root.join(".devcontainer").join("docker-compose.yml");
    fs::create_dir_all(compose_file.parent().expect("compose dir")).expect("compose dir");
    fs::write(&compose_file, "services:\n  app:\n    image: alpine:3.20\n").expect("compose");

    let project_name = compose_project_name(&[compose_file]).expect("project name");

    assert_eq!(
        project_name,
        root.file_name().unwrap().to_string_lossy().to_lowercase() + "_devcontainer"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compose_name_from_file_reads_top_level_name() {
    let root = unique_temp_dir();
    let compose_file = root.join("docker-compose.yml");
    fs::create_dir_all(&root).expect("compose dir");
    fs::write(
        &compose_file,
        "name: Custom-Project-Name\nservices:\n  app:\n    image: alpine:3.20\n",
    )
    .expect("compose");

    let project_name = compose_name_from_file(&compose_file)
        .expect("compose name")
        .expect("top-level name");

    assert_eq!(project_name, "Custom-Project-Name");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compose_name_from_file_supports_colon_dash_default_interpolation() {
    let root = unique_temp_dir();
    let compose_file = root.join("docker-compose.yml");
    let variable = format!("DEVCONTAINER_COMPOSE_TEST_MISSING_{}_A", std::process::id());
    fs::create_dir_all(&root).expect("compose dir");
    fs::write(
        &compose_file,
        format!("name: ${{{variable}:-MyProj}}\nservices:\n  app:\n    image: alpine:3.20\n"),
    )
    .expect("compose");

    let project_name = compose_name_from_file(&compose_file)
        .expect("compose name")
        .expect("top-level name");

    assert_eq!(project_name, "MyProj");
    assert_eq!(sanitize_project_name(&project_name), "myproj");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compose_name_from_file_supports_dash_default_interpolation() {
    let root = unique_temp_dir();
    let compose_file = root.join("docker-compose.yml");
    let variable = format!("DEVCONTAINER_COMPOSE_TEST_MISSING_{}_B", std::process::id());
    fs::create_dir_all(&root).expect("compose dir");
    fs::write(
        &compose_file,
        format!("name: ${{{variable}-MyProj}}\nservices:\n  app:\n    image: alpine:3.20\n"),
    )
    .expect("compose");

    let project_name = compose_name_from_file(&compose_file)
        .expect("compose name")
        .expect("top-level name");

    assert_eq!(project_name, "MyProj");
    assert_eq!(sanitize_project_name(&project_name), "myproj");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn substitute_compose_env_supports_plain_variable_interpolation() {
    let variable = format!("DEVCONTAINER_COMPOSE_TEST_PRESENT_{}", std::process::id());
    unsafe {
        std::env::set_var(&variable, "MyProject");
    }

    assert_eq!(
        substitute_compose_env(&format!("prefix-${variable}")),
        "prefix-MyProject"
    );

    unsafe {
        std::env::remove_var(variable);
    }
}

#[test]
fn parse_semver_prefix_reads_plain_semver_versions() {
    assert_eq!(parse_semver_prefix("2.24.0"), Some((2, 24, 0)));
    assert_eq!(parse_semver_prefix("v2.8.1-desktop.1"), Some((2, 8, 1)));
}

#[test]
fn compose_image_name_separator_defaults_to_hyphen_without_runtime_args() {
    assert_eq!(compose_image_name_separator(&[]), '-');
}

#[test]
fn metadata_override_file_mounts_workspace_by_default() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("workspace root");
    let resolved = crate::runtime::context::ResolvedConfig {
        workspace_folder: root.clone(),
        config_file: root.join(".devcontainer.json"),
        configuration: json!({
            "dockerComposeFile": "docker-compose.yml",
            "service": "app",
        }),
    };

    let override_file = compose_metadata_override_file(&resolved, &[], "/workspaces/project", None)
        .expect("override result")
        .expect("override path");
    let override_content = fs::read_to_string(&override_file).expect("override content");

    assert!(override_content.contains("volumes:"));
    assert!(override_content.contains(&format!("- '{}:/workspaces/project'", root.display())));

    let _ = fs::remove_file(override_file);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn metadata_override_file_mounts_nested_workspaces_from_the_git_root() {
    let root = unique_temp_dir();
    let repo_root = root.join("repo");
    let workspace = repo_root.join("packages").join("app");
    fs::create_dir_all(&workspace).expect("workspace root");
    init_git_repo(&repo_root);
    let expected_repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.clone());
    let resolved = crate::runtime::context::ResolvedConfig {
        workspace_folder: workspace,
        config_file: expected_repo_root
            .join("packages")
            .join("app")
            .join(".devcontainer.json"),
        configuration: json!({
            "dockerComposeFile": "docker-compose.yml",
            "service": "app",
        }),
    };

    let override_file =
        compose_metadata_override_file(&resolved, &[], "/workspaces/repo/packages/app", None)
            .expect("override result")
            .expect("override path");
    let override_content = fs::read_to_string(&override_file).expect("override content");

    assert!(override_content.contains(&format!(
        "- '{}:/workspaces/repo'",
        expected_repo_root.display()
    )));
    assert!(!override_content.contains(&format!(
        "{}:/workspaces/repo/packages/app",
        expected_repo_root.display()
    )));

    let _ = fs::remove_file(override_file);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn metadata_override_file_can_pin_image_and_runtime_settings() {
    let root = unique_temp_dir();
    fs::create_dir_all(&root).expect("workspace root");
    let resolved = crate::runtime::context::ResolvedConfig {
        workspace_folder: root.clone(),
        config_file: root.join(".devcontainer.json"),
        configuration: json!({
            "dockerComposeFile": "docker-compose.yml",
            "service": "app",
            "containerEnv": {
                "FEATURE_FLAG": "enabled"
            },
            "remoteUser": "vscode",
            "privileged": true,
            "init": true,
            "capAdd": ["SYS_ADMIN"],
            "securityOpt": ["seccomp=unconfined"],
            "mounts": [{
                "type": "volume",
                "source": "feature-cache",
                "target": "/cache"
            }, "type=bind,source=/tmp/feature-src,target=/tmp/feature-dst,readonly"]
        }),
    };

    let override_file = compose_metadata_override_file(
        &resolved,
        &[],
        "/workspaces/project",
        Some("example/compose-featured:test"),
    )
    .expect("override result")
    .expect("override path");
    let override_content = fs::read_to_string(&override_file).expect("override content");

    assert!(override_content.contains("image: 'example/compose-featured:test'"));
    assert!(override_content.contains("environment:"));
    assert!(override_content.contains("FEATURE_FLAG: 'enabled'"));
    assert!(override_content.contains("user: 'vscode'"));
    assert!(override_content.contains("privileged: true"));
    assert!(override_content.contains("init: true"));
    assert!(override_content.contains("cap_add:"));
    assert!(override_content.contains("security_opt:"));
    assert!(override_content.contains("type: 'volume'"));
    assert!(override_content.contains("source: 'feature-cache'"));
    assert!(override_content.contains("target: '/cache'"));
    assert!(override_content.contains("type: 'bind'"));
    assert!(override_content.contains("source: '/tmp/feature-src'"));
    assert!(override_content.contains("target: '/tmp/feature-dst'"));
    assert!(override_content.contains("read_only: true"));
    assert!(!override_content.contains("type=volume,source=feature-cache,target=/cache"));
    assert!(!override_content
        .contains("type=bind,source=/tmp/feature-src,target=/tmp/feature-dst,readonly"));

    let _ = fs::remove_file(override_file);
    let _ = fs::remove_dir_all(root);
}
