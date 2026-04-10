//! YAML rendering helpers for compose override files.

use serde_json::{Map, Value};

use super::override_mounts::ComposeVolumeEntry;

pub(super) fn escape_compose_label(label: &str) -> String {
    label.replace('\'', "''").replace('$', "$$")
}

pub(super) fn escape_compose_scalar(value: &str) -> String {
    value.replace('\'', "''")
}

pub(super) fn render_compose_volume_entry(entry: &ComposeVolumeEntry) -> String {
    match entry {
        ComposeVolumeEntry::Short(volume) => {
            format!("      - '{}'\n", escape_compose_scalar(volume))
        }
        ComposeVolumeEntry::Long(definition) => render_yaml_mapping_list_entry(&definition.fields),
    }
}

fn render_yaml_mapping_list_entry(entries: &Map<String, Value>) -> String {
    let mut rendered = String::new();
    let mut iter = entries.iter();
    if let Some((key, value)) = iter.next() {
        rendered.push_str(&render_yaml_key_value(key, value, 6, "- "));
    }
    for (key, value) in iter {
        rendered.push_str(&render_yaml_key_value(key, value, 8, ""));
    }
    rendered
}

fn render_yaml_key_value(key: &str, value: &Value, indent: usize, prefix: &str) -> String {
    let padding = " ".repeat(indent);
    match value {
        Value::Object(entries) => {
            let mut rendered = format!("{padding}{prefix}{key}:\n");
            let nested_indent = indent + prefix.len() + 2;
            for (nested_key, nested_value) in entries {
                rendered.push_str(&render_yaml_key_value(
                    nested_key,
                    nested_value,
                    nested_indent,
                    "",
                ));
            }
            rendered
        }
        Value::Array(values) => {
            let mut rendered = format!("{padding}{prefix}{key}:\n");
            let nested_indent = indent + prefix.len() + 2;
            for nested_value in values {
                rendered.push_str(&render_yaml_sequence_item(nested_value, nested_indent));
            }
            rendered
        }
        Value::String(text) => format!(
            "{padding}{prefix}{key}: '{}'\n",
            escape_compose_scalar(text)
        ),
        Value::Bool(boolean) => format!(
            "{padding}{prefix}{key}: {}\n",
            if *boolean { "true" } else { "false" }
        ),
        Value::Number(number) => format!("{padding}{prefix}{key}: {number}\n"),
        Value::Null => format!("{padding}{prefix}{key}: null\n"),
    }
}

fn render_yaml_sequence_item(value: &Value, indent: usize) -> String {
    let padding = " ".repeat(indent);
    match value {
        Value::Object(entries) => {
            let mut rendered = String::new();
            let mut iter = entries.iter();
            if let Some((key, value)) = iter.next() {
                rendered.push_str(&render_yaml_key_value(key, value, indent, "- "));
            }
            for (key, value) in iter {
                rendered.push_str(&render_yaml_key_value(key, value, indent + 2, ""));
            }
            rendered
        }
        Value::Array(values) => {
            let mut rendered = format!("{padding}-\n");
            for nested_value in values {
                rendered.push_str(&render_yaml_sequence_item(nested_value, indent + 2));
            }
            rendered
        }
        Value::String(text) => format!("{padding}- '{}'\n", escape_compose_scalar(text)),
        Value::Bool(boolean) => {
            format!("{padding}- {}\n", if *boolean { "true" } else { "false" })
        }
        Value::Number(number) => format!("{padding}- {number}\n"),
        Value::Null => format!("{padding}- null\n"),
    }
}
