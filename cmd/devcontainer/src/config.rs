//! Dev container config parsing, path resolution, and variable substitution.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct ConfigContext {
    pub workspace_folder: PathBuf,
    pub env: HashMap<String, String>,
    pub container_workspace_folder: Option<String>,
    pub id_labels: HashMap<String, String>,
}

pub fn resolve_config_path(
    workspace_folder: &Path,
    explicit_config: Option<&Path>,
) -> Result<PathBuf, String> {
    let config_path = if let Some(config) = explicit_config {
        expected_config_path(workspace_folder, Some(config))
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

pub fn expected_config_path(workspace_folder: &Path, explicit_config: Option<&Path>) -> PathBuf {
    if let Some(config) = explicit_config {
        if config.is_absolute() {
            config.to_path_buf()
        } else {
            workspace_folder.join(config)
        }
    } else {
        workspace_folder
            .join(".devcontainer")
            .join("devcontainer.json")
    }
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

fn replace_variable(
    variable: &str,
    context: Option<&ConfigContext>,
    container_env: Option<&HashMap<String, String>>,
) -> Option<String> {
    let (name, args) = parse_variable(variable);
    match name {
        "localWorkspaceFolder" => {
            context.map(|value| value.workspace_folder.to_string_lossy().into_owned())
        }
        "localWorkspaceFolderBasename" => context.map(|value| {
            value
                .workspace_folder
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| value.workspace_folder.to_string_lossy().into_owned())
        }),
        "containerWorkspaceFolder" => {
            context.and_then(|value| value.container_workspace_folder.clone())
        }
        "containerWorkspaceFolderBasename" => context.and_then(|value| {
            value
                .container_workspace_folder
                .as_deref()
                .and_then(|folder| Path::new(folder).file_name())
                .and_then(|name| name.to_str())
                .map(str::to_string)
        }),
        "devcontainerId" => context
            .filter(|value| !value.id_labels.is_empty())
            .map(|value| devcontainer_id_for_labels(&value.id_labels)),
        "env" | "localEnv" => context.and_then(|value| {
            args.first().map(|variable_name| {
                value
                    .env
                    .get(*variable_name)
                    .cloned()
                    .or_else(|| args.get(1).map(|default| (*default).to_string()))
                    .unwrap_or_default()
            })
        }),
        "containerEnv" => container_env.and_then(|value| {
            args.first().map(|variable_name| {
                value
                    .get(*variable_name)
                    .cloned()
                    .or_else(|| args.get(1).map(|default| (*default).to_string()))
                    .unwrap_or_default()
            })
        }),
        _ => None,
    }
}

fn substitute_string(
    input: &str,
    context: Option<&ConfigContext>,
    container_env: Option<&HashMap<String, String>>,
) -> String {
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
        if let Some(replacement) = replace_variable(variable, context, container_env) {
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

fn devcontainer_id_for_labels(labels: &HashMap<String, String>) -> String {
    let mut entries = labels.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    let serialized = format!(
        "{{{}}}",
        entries
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "{}:{}",
                    serde_json::to_string(key).unwrap_or_default(),
                    serde_json::to_string(value).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    );
    let hash = Sha256::digest(serialized.as_bytes());
    encode_base32hex_lower(&hash)
}

fn encode_base32hex_lower(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";

    let mut output = String::new();
    let mut buffer = 0_u16;
    let mut bits = 0_u8;
    for byte in bytes {
        buffer = (buffer << 8) | u16::from(*byte);
        bits += 8;
        while bits >= 5 {
            let index = ((buffer >> (bits - 5)) & 0x1f) as usize;
            output.push(ALPHABET[index] as char);
            bits -= 5;
            if bits == 0 {
                buffer = 0;
            } else {
                buffer &= (1 << bits) - 1;
            }
        }
    }
    if bits > 0 {
        let index = ((buffer << (5 - bits)) & 0x1f) as usize;
        output.push(ALPHABET[index] as char);
    }
    while output.len() < 52 {
        output.insert(0, '0');
    }
    output
}

pub fn substitute_local_context(value: &Value, context: &ConfigContext) -> Value {
    let mut resolved_context = context.clone();
    if let Some(container_workspace_folder) = context.container_workspace_folder.as_deref() {
        resolved_context.container_workspace_folder = Some(substitute_string(
            container_workspace_folder,
            Some(&ConfigContext {
                workspace_folder: context.workspace_folder.clone(),
                env: context.env.clone(),
                container_workspace_folder: None,
                id_labels: context.id_labels.clone(),
            }),
            None,
        ));
    }
    substitute_local_context_resolved(value, &resolved_context)
}

fn substitute_local_context_resolved(value: &Value, context: &ConfigContext) -> Value {
    match value {
        Value::String(text) => Value::String(substitute_string(text, Some(context), None)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute_local_context_resolved(item, context))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        substitute_local_context_resolved(value, context),
                    )
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

pub fn substitute_container_env(value: &Value, env: &HashMap<String, String>) -> Value {
    match value {
        Value::String(text) => Value::String(substitute_string(text, None, Some(env))),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute_container_env(item, env))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), substitute_container_env(value, env)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for config parsing and substitution behavior.

    use super::{
        parse_jsonc_value, resolve_config_path, substitute_container_env, substitute_local_context,
        ConfigContext,
    };
    use crate::test_support::unique_temp_dir;
    use serde_json::json;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn discovers_standard_devcontainer_config_path() {
        let root = unique_temp_dir("devcontainer-config-test");
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
            container_workspace_folder: Some("/workspaces/demo".to_string()),
            id_labels: HashMap::new(),
        };
        let value = json!({
            "containerEnv": {
                "USER_NAME": "${localEnv:USER}",
                "WORKSPACE": "${localWorkspaceFolder}",
                "CONTAINER_WORKSPACE": "${containerWorkspaceFolder}",
                "CONTAINER_BASENAME": "${containerWorkspaceFolderBasename}"
            }
        });

        let substituted = substitute_local_context(&value, &context);

        assert_eq!(substituted["containerEnv"]["USER_NAME"], "johan");
        assert_eq!(substituted["containerEnv"]["WORKSPACE"], "/workspace/demo");
        assert_eq!(
            substituted["containerEnv"]["CONTAINER_WORKSPACE"],
            "/workspaces/demo"
        );
        assert_eq!(substituted["containerEnv"]["CONTAINER_BASENAME"], "demo");
    }

    #[test]
    fn substitutes_workspace_basename_and_defaulted_env_tokens() {
        let context = ConfigContext {
            workspace_folder: PathBuf::from("/workspace/demo"),
            env: HashMap::new(),
            container_workspace_folder: Some("/workspaces/${localWorkspaceFolderBasename}".into()),
            id_labels: HashMap::new(),
        };
        let value = json!({
            "containerEnv": {
                "BASENAME": "${localWorkspaceFolderBasename}",
                "DEFAULTED": "${localEnv:USER:fallback}",
                "DEFAULT_WITH_EXTRA_SEGMENTS": "${env:USER:fallback:ignored}",
                "MISSING": "before-${localEnv:UNSET}-after",
                "CONTAINER_PATH": "${containerWorkspaceFolder}"
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
        assert_eq!(
            substituted["containerEnv"]["CONTAINER_PATH"],
            "/workspaces/demo"
        );
    }

    #[test]
    fn substitutes_container_env_tokens_without_replacing_local_env_tokens() {
        let value = json!({
            "remoteEnv": {
                "PATH_FROM_CONTAINER": "${containerEnv:PATH}",
                "FALLBACK": "${containerEnv:MISSING:fallback}",
                "LOCAL_PATH": "${localEnv:PATH}"
            }
        });
        let substituted = substitute_container_env(
            &value,
            &HashMap::from([("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string())]),
        );

        assert_eq!(
            substituted["remoteEnv"]["PATH_FROM_CONTAINER"],
            "/usr/local/bin:/usr/bin"
        );
        assert_eq!(substituted["remoteEnv"]["FALLBACK"], "fallback");
        assert_eq!(substituted["remoteEnv"]["LOCAL_PATH"], "${localEnv:PATH}");
    }

    #[test]
    fn substitutes_stable_devcontainer_id_from_sorted_labels() {
        let value = json!({
            "mounts": [
                {
                    "source": "cache-${devcontainerId}",
                    "target": "/cache",
                    "type": "volume"
                }
            ]
        });
        let first = substitute_local_context(
            &value,
            &ConfigContext {
                workspace_folder: PathBuf::from("/workspace/demo"),
                env: HashMap::new(),
                container_workspace_folder: None,
                id_labels: HashMap::from([
                    ("b".to_string(), "2".to_string()),
                    ("a".to_string(), "1".to_string()),
                ]),
            },
        );
        let second = substitute_local_context(
            &value,
            &ConfigContext {
                workspace_folder: PathBuf::from("/workspace/demo"),
                env: HashMap::new(),
                container_workspace_folder: None,
                id_labels: HashMap::from([
                    ("a".to_string(), "1".to_string()),
                    ("b".to_string(), "2".to_string()),
                ]),
            },
        );
        let id = first["mounts"][0]["source"]
            .as_str()
            .expect("mount source")
            .trim_start_matches("cache-")
            .to_string();

        assert_eq!(first, second);
        assert_eq!(id.len(), 52);
        assert!(id
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='v')));
    }
}
