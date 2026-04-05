use std::io::{self, Write};
use std::process::ExitCode;

use crate::runtime;

pub(crate) fn run(args: &[String]) -> ExitCode {
    match runtime::run_exec(args) {
        Ok(result) => {
            let _ = io::stdout().write_all(result.stdout.as_bytes());
            let _ = io::stderr().write_all(result.stderr.as_bytes());
            ExitCode::from(result.status_code as u8)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
