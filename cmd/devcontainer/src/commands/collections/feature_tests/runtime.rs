//! Runtime execution helpers for feature test commands.

use std::fs;
use std::path::Path;

use super::discovery::prepare_feature_test_case;
use super::materialize::{
    shell_single_quote, unique_feature_test_name, write_feature_test_dockerfile,
};
use super::{BaseImageSource, FeatureTestCase, FeatureTestOptions, FeatureTestResult};
use crate::runtime;

pub(crate) trait FeatureTestRuntime {
    fn build_image(
        &mut self,
        args: &[String],
        image_name: &str,
        dockerfile_path: &Path,
        context_path: &Path,
    ) -> Result<(), String>;
    fn start_container(
        &mut self,
        args: &[String],
        image_name: &str,
        workspace_dir: &Path,
    ) -> Result<String, String>;
    fn exec_script(
        &mut self,
        args: &[String],
        container_id: &str,
        workspace_dir: &Path,
        remote_user: Option<&str>,
        env: &[(String, String)],
        script_name: &str,
    ) -> Result<i32, String>;
    fn remove_container(&mut self, args: &[String], container_id: &str) -> Result<(), String>;
}

pub(super) struct ContainerEngineFeatureTestRuntime;

impl FeatureTestRuntime for ContainerEngineFeatureTestRuntime {
    fn build_image(
        &mut self,
        args: &[String],
        image_name: &str,
        dockerfile_path: &Path,
        context_path: &Path,
    ) -> Result<(), String> {
        let result = runtime::engine::run_engine(
            args,
            vec![
                "build".to_string(),
                "--tag".to_string(),
                image_name.to_string(),
                "--file".to_string(),
                dockerfile_path.display().to_string(),
                context_path.display().to_string(),
            ],
        )?;
        if result.status_code != 0 {
            return Err(runtime::engine::stderr_or_stdout(&result));
        }
        Ok(())
    }

    fn start_container(
        &mut self,
        args: &[String],
        image_name: &str,
        workspace_dir: &Path,
    ) -> Result<String, String> {
        let result = runtime::engine::run_engine(
            args,
            vec![
                "run".to_string(),
                "-d".to_string(),
                "--label".to_string(),
                "devcontainer.is_test_run=true".to_string(),
                "--mount".to_string(),
                format!(
                    "type=bind,source={},target=/workspace",
                    workspace_dir.display()
                ),
                "--workdir".to_string(),
                "/workspace".to_string(),
                image_name.to_string(),
                "/bin/sh".to_string(),
                "-lc".to_string(),
                "while sleep 1000; do :; done".to_string(),
            ],
        )?;
        if result.status_code != 0 {
            return Err(runtime::engine::stderr_or_stdout(&result));
        }
        Ok(result.stdout.trim().to_string())
    }

    fn exec_script(
        &mut self,
        args: &[String],
        container_id: &str,
        _workspace_dir: &Path,
        remote_user: Option<&str>,
        env: &[(String, String)],
        script_name: &str,
    ) -> Result<i32, String> {
        let mut engine_args = vec![
            "exec".to_string(),
            "--workdir".to_string(),
            "/workspace".to_string(),
        ];
        if let Some(remote_user) = remote_user {
            engine_args.push("--user".to_string());
            engine_args.push(remote_user.to_string());
        }
        for (key, value) in env {
            engine_args.push("-e".to_string());
            engine_args.push(format!("{key}={value}"));
        }
        engine_args.push(container_id.to_string());
        engine_args.push("/bin/bash".to_string());
        engine_args.push("-lc".to_string());
        engine_args.push(format!(
            "chmod -R 777 /workspace && {}",
            shell_single_quote(&format!("./{script_name}"))
        ));
        runtime::engine::run_engine_streaming(args, engine_args)
    }

    fn remove_container(&mut self, args: &[String], container_id: &str) -> Result<(), String> {
        let result = runtime::engine::run_engine(
            args,
            vec!["rm".to_string(), "-f".to_string(), container_id.to_string()],
        )?;
        if result.status_code != 0 {
            return Err(runtime::engine::stderr_or_stdout(&result));
        }
        Ok(())
    }
}

pub(super) fn execute_feature_tests_with_runtime<R: FeatureTestRuntime>(
    args: &[String],
    runtime: &mut R,
    options: &FeatureTestOptions,
    cases: Vec<FeatureTestCase>,
) -> Result<Vec<FeatureTestResult>, String> {
    let mut results = Vec::with_capacity(cases.len());

    for case in cases {
        let prepared = prepare_feature_test_case(options, &case)?;
        let base_image = match &prepared.base_image {
            BaseImageSource::Image(image) => image.clone(),
            BaseImageSource::Build {
                dockerfile_path,
                context_path,
            } => {
                let image_name = unique_feature_test_name("devcontainer-feature-test-base");
                runtime.build_image(args, &image_name, dockerfile_path, context_path)?;
                image_name
            }
        };
        let dockerfile_path = write_feature_test_dockerfile(
            &prepared.build_context_dir,
            &base_image,
            &prepared.feature_installations,
        )?;
        let image_name = unique_feature_test_name("devcontainer-feature-test");
        runtime.build_image(
            args,
            &image_name,
            &dockerfile_path,
            &prepared.build_context_dir,
        )?;
        let container_id = runtime.start_container(args, &image_name, &prepared.workspace_dir)?;
        let status = runtime.exec_script(
            args,
            &container_id,
            &prepared.workspace_dir,
            prepared.remote_user.as_deref(),
            &prepared.exec_env,
            &prepared.script_name,
        )?;
        if !options.preserve_test_containers {
            runtime.remove_container(args, &container_id)?;
            let _ = fs::remove_dir_all(&prepared.workspace_dir);
        }
        results.push(FeatureTestResult {
            name: case.name,
            passed: status == 0,
        });
    }

    Ok(results)
}
