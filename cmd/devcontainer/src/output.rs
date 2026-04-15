//! Output rendering helpers for JSON and human-readable command results.

#[derive(Clone, Copy)]
pub enum LogFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandLogLevel {
    Trace,
    Debug,
    Info,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalDimensions {
    pub columns: usize,
    pub rows: usize,
}

pub struct CommandLogger {
    format: LogFormat,
    level: CommandLogLevel,
    terminal_dimensions: Option<TerminalDimensions>,
}

impl CommandLogLevel {
    fn severity(self) -> u8 {
        match self {
            Self::Trace => 0,
            Self::Debug => 1,
            Self::Info => 2,
            Self::Error => 3,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Error => "error",
        }
    }
}

impl CommandLogger {
    pub fn new(format: LogFormat, level: CommandLogLevel) -> Self {
        Self {
            format,
            level,
            terminal_dimensions: None,
        }
    }

    pub fn with_terminal_dimensions(
        mut self,
        terminal_dimensions: Option<TerminalDimensions>,
    ) -> Self {
        self.terminal_dimensions = terminal_dimensions;
        self
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.log(CommandLogLevel::Error, message);
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.log(CommandLogLevel::Info, message);
    }

    pub fn debug(&self, message: impl AsRef<str>) {
        self.log(CommandLogLevel::Debug, message);
    }

    pub fn trace(&self, message: impl AsRef<str>) {
        self.log(CommandLogLevel::Trace, message);
    }

    pub fn trace_terminal_dimensions(&self) {
        if let Some(dimensions) = self.terminal_dimensions {
            self.trace(format!(
                "Using terminal dimensions: columns={} rows={}",
                dimensions.columns, dimensions.rows
            ));
        }
    }

    fn log(&self, level: CommandLogLevel, message: impl AsRef<str>) {
        if let Some(rendered) = self.render(level, message.as_ref()) {
            eprintln!("{rendered}");
        }
    }

    fn render(&self, level: CommandLogLevel, message: &str) -> Option<String> {
        if level.severity() < self.level.severity() {
            return None;
        }

        Some(match self.format {
            LogFormat::Text => format!("[{}] {message}", level.as_str()),
            LogFormat::Json => render_log(self.format, level.as_str(), message),
        })
    }
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
    use super::{render_log, CommandLogLevel, CommandLogger, LogFormat, TerminalDimensions};

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

    #[test]
    fn command_logger_filters_entries_below_configured_level() {
        let logger = CommandLogger::new(LogFormat::Text, CommandLogLevel::Info);

        assert_eq!(logger.render(CommandLogLevel::Trace, "ignored"), None);
        assert_eq!(
            logger.render(CommandLogLevel::Error, "emitted"),
            Some("[error] emitted".to_string())
        );
    }

    #[test]
    fn command_logger_renders_json_entries() {
        let logger = CommandLogger::new(LogFormat::Json, CommandLogLevel::Trace);

        assert_eq!(
            logger.render(CommandLogLevel::Debug, "quoted \"value\""),
            Some("{\"level\":\"debug\",\"message\":\"quoted \\\"value\\\"\"}".to_string())
        );
    }

    #[test]
    fn command_logger_stores_terminal_dimensions() {
        let logger = CommandLogger::new(LogFormat::Text, CommandLogLevel::Trace)
            .with_terminal_dimensions(Some(TerminalDimensions {
                columns: 120,
                rows: 40,
            }));

        assert_eq!(
            logger.terminal_dimensions,
            Some(TerminalDimensions {
                columns: 120,
                rows: 40,
            })
        );
    }
}
