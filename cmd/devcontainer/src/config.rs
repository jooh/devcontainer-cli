use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

pub struct ConfigContext {
    pub workspace_folder: PathBuf,
    pub env: HashMap<String, String>,
}

pub fn resolve_config_path(
    workspace_folder: &Path,
    explicit_config: Option<&Path>,
) -> Result<PathBuf, String> {
    let config_path = if let Some(config) = explicit_config {
        if config.is_absolute() {
            config.to_path_buf()
        } else {
            workspace_folder.join(config)
        }
    } else {
        let modern = workspace_folder
            .join(".devcontainer")
            .join("devcontainer.json");
        let legacy = workspace_folder.join(".devcontainer.json");
        if modern.is_file() {
            modern
        } else {
            legacy
        }
    };

    if !config_path.is_file() {
        return Err(format!(
            "Unable to locate a dev container config at {}",
            config_path.display()
        ));
    }

    Ok(fs::canonicalize(&config_path).unwrap_or(config_path))
}

fn strip_jsonc_comments(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(current) = chars.next() {
        let next = chars.peek().copied();

        if in_line_comment {
            if current == '\n' {
                in_line_comment = false;
                result.push(current);
            }
            continue;
        }

        if in_block_comment {
            if current == '*' && next == Some('/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if in_string {
            result.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            continue;
        }

        if current == '"' {
            in_string = true;
            result.push(current);
            continue;
        }

        if current == '/' && next == Some('/') {
            chars.next();
            in_line_comment = true;
            continue;
        }

        if current == '/' && next == Some('*') {
            chars.next();
            in_block_comment = true;
            continue;
        }

        result.push(current);
    }

    result
}

fn strip_trailing_commas(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let characters: Vec<char> = text.chars().collect();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;

    while index < characters.len() {
        let current = characters[index];

        if in_string {
            result.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if current == '"' {
            in_string = true;
            result.push(current);
            index += 1;
            continue;
        }

        if current == ',' {
            let mut lookahead = index + 1;
            while lookahead < characters.len() && characters[lookahead].is_whitespace() {
                lookahead += 1;
            }

            if lookahead < characters.len()
                && (characters[lookahead] == '}' || characters[lookahead] == ']')
            {
                index += 1;
                continue;
            }
        }

        result.push(current);
        index += 1;
    }

    result
}

pub fn parse_jsonc_value(text: &str) -> Result<Value, String> {
    let sanitized = strip_trailing_commas(&strip_jsonc_comments(text));
    serde_json::from_str(&sanitized).map_err(|error| error.to_string())
}

fn parse_variable(input: &str) -> (&str, Vec<&str>) {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() > 1 {
        (parts[0], parts[1..].to_vec())
    } else {
        (input, Vec::new())
    }
}

fn replace_variable(variable: &str, context: &ConfigContext) -> Option<String> {
    let (name, args) = parse_variable(variable);
    match name {
        "localWorkspaceFolder" => Some(context.workspace_folder.to_string_lossy().into_owned()),
        "localWorkspaceFolderBasename" => Some(
            context
                .workspace_folder
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| context.workspace_folder.to_string_lossy().into_owned()),
        ),
        "env" | "localEnv" => args.first().map(|variable_name| {
            context
                .env
                .get(*variable_name)
                .cloned()
                .or_else(|| args.get(1).map(|value| (*value).to_string()))
                .unwrap_or_default()
        }),
        _ => None,
    }
}

fn substitute_string(input: &str, context: &ConfigContext) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);

        let variable_start = start + 2;
        let after_start = &remaining[variable_start..];
        let Some(end_offset) = after_start.find('}') else {
            output.push_str(&remaining[start..]);
            return output;
        };

        let variable = &after_start[..end_offset];
        if let Some(replacement) = replace_variable(variable, context) {
            output.push_str(&replacement);
        } else {
            output.push_str("${");
            output.push_str(variable);
            output.push('}');
        }

        remaining = &after_start[end_offset + 1..];
    }

    output.push_str(remaining);
    output
}

pub fn substitute_local_context(value: &Value, context: &ConfigContext) -> Value {
    match value {
        Value::String(text) => Value::String(substitute_string(text, context)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute_local_context(item, context))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), substitute_local_context(value, context)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_jsonc_value, resolve_config_path, substitute_local_context, ConfigContext};
    use serde_json::json;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("devcontainer-config-test-{suffix}"))
    }

    #[test]
    fn discovers_standard_devcontainer_config_path() {
        let root = unique_temp_dir();
        let config_dir = root.join(".devcontainer");
        let config_path = config_dir.join("devcontainer.json");
        fs::create_dir_all(&config_dir).expect("failed to create config directory");
        fs::write(&config_path, "{ \"image\": \"example\" }").expect("failed to write config");

        let resolved = resolve_config_path(&root, None).expect("expected config path");

        assert_eq!(
            resolved,
            fs::canonicalize(config_path).expect("failed to canonicalize")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_jsonc_with_comments_and_trailing_commas() {
        let parsed = parse_jsonc_value("{\n  // comment\n  \"name\": \"demo\",\n}\n")
            .expect("expected parse");
        assert_eq!(parsed["name"], "demo");
    }

    #[test]
    fn substitutes_local_env_and_workspace_tokens() {
        let mut env = HashMap::new();
        env.insert("USER".to_string(), "johan".to_string());
        let context = ConfigContext {
            workspace_folder: PathBuf::from("/workspace/demo"),
            env,
        };
        let value = json!({
            "containerEnv": {
                "USER_NAME": "${localEnv:USER}",
                "WORKSPACE": "${localWorkspaceFolder}"
            }
        });

        let substituted = substitute_local_context(&value, &context);

        assert_eq!(substituted["containerEnv"]["USER_NAME"], "johan");
        assert_eq!(substituted["containerEnv"]["WORKSPACE"], "/workspace/demo");
    }

    #[test]
    fn substitutes_workspace_basename_and_defaulted_env_tokens() {
        let context = ConfigContext {
            workspace_folder: PathBuf::from("/workspace/demo"),
            env: HashMap::new(),
        };
        let value = json!({
            "containerEnv": {
                "BASENAME": "${localWorkspaceFolderBasename}",
                "DEFAULTED": "${localEnv:USER:fallback}",
                "DEFAULT_WITH_EXTRA_SEGMENTS": "${env:USER:fallback:ignored}",
                "MISSING": "before-${localEnv:UNSET}-after"
            }
        });

        let substituted = substitute_local_context(&value, &context);

        assert_eq!(substituted["containerEnv"]["BASENAME"], "demo");
        assert_eq!(substituted["containerEnv"]["DEFAULTED"], "fallback");
        assert_eq!(
            substituted["containerEnv"]["DEFAULT_WITH_EXTRA_SEGMENTS"],
            "fallback"
        );
        assert_eq!(substituted["containerEnv"]["MISSING"], "before--after");
    }
}
