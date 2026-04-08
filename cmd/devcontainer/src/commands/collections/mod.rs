mod feature_tests;
mod features;
mod publish;
pub(crate) mod registry;
mod templates;

use std::process::ExitCode;

use serde_json::Value;

use crate::commands::common;

pub(crate) fn run_features(args: &[String]) -> ExitCode {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let result = match subcommand {
        "list" | "ls" => {
            print_collection_list("features");
            return ExitCode::SUCCESS;
        }
        "resolve-dependencies" => features::build_features_resolve_dependencies_payload(&args[1..]),
        "info" => {
            if args.len() < 3 {
                Err("features info requires manifest <feature>".to_string())
            } else {
                let _ = common::parse_option_value(&args[3..], "--log-level");
                match features::build_feature_info_payload(&args[1], &args[2]) {
                    Ok(payload)
                        if common::parse_option_value(&args[3..], "--output-format").as_deref()
                            == Some("text") =>
                    {
                        println!("{}", render_collection_info_text(&payload));
                        return ExitCode::SUCCESS;
                    }
                    result => result,
                }
            }
        }
        "test" => return feature_tests::run_features_test(&args[1..]),
        "package" => {
            if args.len() < 2 {
                Err("features package requires <target>".to_string())
            } else {
                crate::commands::common::package_collection_target(
                    std::path::Path::new(&args[1]),
                    "devcontainer-feature.json",
                    "feature",
                )
                .map(|archive| {
                    serde_json::json!({
                        "outcome": "success",
                        "command": "features package",
                        "archive": archive,
                    })
                })
            }
        }
        "publish" => {
            if args.len() < 2 {
                Err("features publish requires <target>".to_string())
            } else {
                publish::publish_collection_target_to_oci(
                    std::path::Path::new(&args[1]),
                    "devcontainer-feature.json",
                    "feature",
                    "features publish",
                    &args[2..],
                )
            }
        }
        "generate-docs" => {
            if args.len() < 2 {
                Err("features generate-docs requires <target>".to_string())
            } else {
                crate::commands::common::generate_manifest_docs(
                    std::path::Path::new(&args[1]),
                    "devcontainer-feature.json",
                    "Feature",
                )
                .map(|readme| {
                    serde_json::json!({
                        "outcome": "success",
                        "command": "features generate-docs",
                        "readme": readme,
                    })
                })
            }
        }
        _ => Err(format!("Unsupported features subcommand: {subcommand}")),
    };

    print_result(result)
}

fn render_collection_info_text(payload: &Value) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string())
}

pub(crate) fn run_templates(args: &[String]) -> ExitCode {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let result = match subcommand {
        "list" | "ls" => {
            print_collection_list("templates");
            return ExitCode::SUCCESS;
        }
        "apply" => templates::run_template_apply(&args[1..]),
        "metadata" => {
            if args.len() < 2 {
                Err("templates metadata requires <target>".to_string())
            } else {
                templates::build_template_metadata_payload(&args[1])
            }
        }
        "publish" => {
            if args.len() < 2 {
                Err("templates publish requires <target>".to_string())
            } else {
                publish::publish_collection_target_to_oci(
                    std::path::Path::new(&args[1]),
                    "devcontainer-template.json",
                    "template",
                    "templates publish",
                    &args[2..],
                )
            }
        }
        "generate-docs" => {
            if args.len() < 2 {
                Err("templates generate-docs requires <target>".to_string())
            } else {
                crate::commands::common::generate_manifest_docs(
                    std::path::Path::new(&args[1]),
                    "devcontainer-template.json",
                    "Template",
                )
                .map(|readme| {
                    serde_json::json!({
                        "outcome": "success",
                        "command": "templates generate-docs",
                        "readme": readme,
                    })
                })
            }
        }
        _ => Err(format!("Unsupported templates subcommand: {subcommand}")),
    };

    print_result(result)
}

fn print_result(result: Result<Value, String>) -> ExitCode {
    match result {
        Ok(payload) => {
            println!("{payload}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn print_collection_list(command: &str) {
    let payload = match command {
        "features" => "{\"features\":[]}",
        "templates" => "{\"templates\":[]}",
        _ => "{}",
    };
    println!("{payload}");
}

#[cfg(test)]
mod tests;
