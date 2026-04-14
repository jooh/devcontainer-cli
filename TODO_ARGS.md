# TODO_ARGS

Unsupported CLI args for the current pinned upstream command surface.

- Upstream commit: `39685cf1aa58b5b11e90085bd32562fad61f4103`
- Source: `upstream/src/spec-node/devContainersSpecCLI.ts`

## `outdated`

- `--log-level`: Log level for the --terminal-log-file. When set to trace, the log level for --log-file will also be set to trace.  [choices: "info", "debug", "trace"] [default: "info"]
- `--terminal-columns`: Number of columns to render the output for. This is required for some of the subprocesses to correctly render their output.  [number]
- `--terminal-rows`: Number of rows to render the output for. This is required for some of the subprocesses to correctly render their output.  [number]

## `run-user-commands`

- `--dotfiles-install-command`: The command to run after cloning the dotfiles repository. Defaults to run the first file of `install.sh`, `install`, `bootstrap.sh`, `bootstrap`, `setup.sh` and `setup` found in the dotfiles repository`s root folder.  [string]
- `--dotfiles-target-path`: The path to clone the dotfiles repository to. Defaults to `~/dotfiles`.  [string] [default: "~/dotfiles"]

## `set-up`

- `--dotfiles-install-command`: The command to run after cloning the dotfiles repository. Defaults to run the first file of `install.sh`, `install`, `bootstrap.sh`, `bootstrap`, `setup.sh` and `setup` found in the dotfiles repository`s root folder.  [string]
- `--dotfiles-target-path`: The path to clone the dotfiles repository to. Defaults to `~/dotfiles`.  [string] [default: "~/dotfiles"]

## `up`

- `--dotfiles-install-command`: The command to run after cloning the dotfiles repository. Defaults to run the first file of `install.sh`, `install`, `bootstrap.sh`, `bootstrap`, `setup.sh` and `setup` found in the dotfiles repository`s root folder.  [string]
- `--dotfiles-target-path`: The path to clone the dotfiles repository to. Defaults to `~/dotfiles`.  [string] [default: "~/dotfiles"]
- `--omit-config-remote-env-from-metadata` [hidden upstream option]: No upstream help description available.
- `--omit-syntax-directive` [hidden upstream option]: No upstream help description available.

## `upgrade`

- `--log-level`: Log level.  [choices: "error", "info", "debug", "trace"] [default: "info"]

