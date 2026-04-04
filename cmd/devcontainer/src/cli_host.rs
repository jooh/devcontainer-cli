use std::env;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct CliHost {
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
}

impl CliHost {
    pub fn from_env() -> Result<Self, String> {
        let cwd = env::current_dir().map_err(|error| error.to_string())?;
        let env = env::vars().collect();
        Ok(Self { cwd, env })
    }

    pub fn lookup_command(&self, command: &str) -> Option<PathBuf> {
        if command.contains(std::path::MAIN_SEPARATOR) {
            let candidate = PathBuf::from(command);
            return if candidate.is_file() {
                Some(fs::canonicalize(&candidate).unwrap_or(candidate))
            } else {
                None
            };
        }

        let path_var = self.env.get("PATH")?;
        env::split_paths(path_var).find_map(|segment| {
            let candidate = segment.join(command);
            if candidate.is_file() {
                Some(fs::canonicalize(&candidate).unwrap_or(candidate))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CliHost;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn locates_commands_from_path_entries() {
        let current_exe = std::env::current_exe().expect("current exe");
        let executable_directory = current_exe.parent().expect("executable directory");
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            executable_directory.to_string_lossy().into_owned(),
        );

        let host = CliHost {
            cwd: PathBuf::from("/workspace"),
            env,
        };

        let resolved = host.lookup_command(
            current_exe
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .as_ref(),
        );

        assert_eq!(resolved.expect("expected lookup result"), current_exe);
    }
}
