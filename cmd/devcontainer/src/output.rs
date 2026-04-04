#[derive(Clone, Copy)]
pub enum LogFormat {
    Text,
    Json,
}

pub fn render_log(format: LogFormat, level: &str, message: &str) -> String {
    match format {
        LogFormat::Text => message.to_string(),
        LogFormat::Json => serde_json::json!({
            "level": level,
            "message": message,
        })
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{render_log, LogFormat};

    #[test]
    fn renders_text_logs_without_json_envelope() {
        assert_eq!(render_log(LogFormat::Text, "info", "hello"), "hello");
    }

    #[test]
    fn renders_json_logs_with_level_and_message() {
        assert_eq!(
            render_log(LogFormat::Json, "info", "quoted \"value\""),
            "{\"level\":\"info\",\"message\":\"quoted \\\"value\\\"\"}"
        );
    }
}
