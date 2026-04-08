# Native Parity Inventory

Generated from the pinned upstream CLI command matrix and static source evidence in the Rust implementation.

- Upstream commit: `39685cf1aa58b5b11e90085bd32562fad61f4103`
- Source: `upstream/src/spec-node/devContainersSpecCLI.ts`
- Declared upstream command paths present natively: `20/20`
- Upstream options with a native source reference in mapped files: `105/200`

This report is a static inventory, not a semantic parity proof. A referenced option can still be only partially implemented, and command-level known gaps are called out explicitly below.

## Summary

| Command | Declared | Option refs | Missing refs | Known gaps |
| --- | --- | --- | --- | --- |
| `up` | yes | 22/43 | 21 | 2 |
| `set-up` | yes | 9/20 | 11 | 1 |
| `build` | yes | 17/22 | 5 | 2 |
| `run-user-commands` | yes | 10/27 | 17 | 1 |
| `read-configuration` | yes | 11/18 | 7 | 2 |
| `outdated` | yes | 4/8 | 4 | 1 |
| `upgrade` | yes | 7/8 | 1 | 1 |
| `features` | yes | 0/0 | 0 | 1 |
| `features test` | yes | 10/13 | 3 | 1 |
| `features package` | yes | 0/0 | 0 | 1 |
| `features publish` | yes | 0/0 | 0 | 1 |
| `features info` | yes | 0/2 | 2 | 1 |
| `features resolve-dependencies` | yes | 1/2 | 1 | 1 |
| `features generate-docs` | yes | 1/6 | 5 | 1 |
| `templates` | yes | 0/0 | 0 | 1 |
| `templates apply` | yes | 4/7 | 3 | 1 |
| `templates publish` | yes | 0/0 | 0 | 1 |
| `templates metadata` | yes | 0/1 | 1 | 1 |
| `templates generate-docs` | yes | 1/4 | 3 | 1 |
| `exec` | yes | 8/19 | 11 | 1 |

## `up`

- Description: Create and run dev container
- Declared natively: yes
- Option source references: 22/43
- Missing option references: `buildkit`, `container-data-folder`, `container-session-data-folder`, `container-system-data-folder`, `default-user-env-probe`, `dotfiles-install-command`, `dotfiles-repository`, `dotfiles-target-path`, `gpu-availability`, `log-level`, `mount-git-worktree-common-dir`, `mount-workspace-git-root`, `omit-config-remote-env-from-metadata`, `omit-syntax-directive`, `override-config`, `secrets-file`, `terminal-columns`, `terminal-rows`, `update-remote-user-uid-default`, `user-data-folder`, `workspace-mount-consistency`
- Known gaps: Native runtime now layers Features for image/dockerfile configs, but compose Feature flows are still missing. Several upstream flags remain unimplemented or are only partially honored.

## `set-up`

- Description: Set up an existing container as a dev container
- Declared natively: yes
- Option source references: 9/20
- Missing option references: `container-data-folder`, `container-session-data-folder`, `container-system-data-folder`, `default-user-env-probe`, `dotfiles-install-command`, `dotfiles-repository`, `dotfiles-target-path`, `log-level`, `terminal-columns`, `terminal-rows`, `user-data-folder`
- Known gaps: Lifecycle execution is native, but upstream secrets and some setup flags are still missing.

## `build`

- Description: Build a dev container image
- Declared natively: yes
- Option source references: 17/22
- Missing option references: `buildkit`, `log-level`, `omit-syntax-directive`, `skip-persisting-customizations-from-features`, `user-data-folder`
- Known gaps: Native runtime now layers Features for image/dockerfile configs, but compose Feature flows are still missing. Several upstream build flags are still unimplemented or are only partially honored.

## `run-user-commands`

- Description: Run user commands
- Declared natively: yes
- Option source references: 10/27
- Missing option references: `container-data-folder`, `container-session-data-folder`, `container-system-data-folder`, `default-user-env-probe`, `dotfiles-install-command`, `dotfiles-repository`, `dotfiles-target-path`, `log-level`, `mount-git-worktree-common-dir`, `mount-workspace-git-root`, `override-config`, `secrets-file`, `skip-feature-auto-mapping`, `stop-for-personalization`, `terminal-columns`, `terminal-rows`, `user-data-folder`
- Known gaps: Lifecycle execution is native, but upstream secrets and some runtime flags are still missing.

## `read-configuration`

- Description: Read configuration
- Declared natively: yes
- Option source references: 11/18
- Missing option references: `log-level`, `mount-git-worktree-common-dir`, `mount-workspace-git-root`, `override-config`, `terminal-columns`, `terminal-rows`, `user-data-folder`
- Known gaps: `--include-features-configuration` resolves local/published Feature sets natively, but still relies on fixture/manual manifests rather than full OCI resolution. Variable substitution support is still narrower than upstream.

## `outdated`

- Description: Show current and available versions
- Declared natively: yes
- Option source references: 4/8
- Missing option references: `log-level`, `terminal-columns`, `terminal-rows`, `user-data-folder`
- Known gaps: Backed by fixture/manual catalog data rather than real upstream registry resolution.

## `upgrade`

- Description: Upgrade lockfile
- Declared natively: yes
- Option source references: 7/8
- Missing option references: `log-level`
- Known gaps: Backed by fixture/manual catalog data rather than real upstream registry resolution.

## `features`

- Description: Features commands
- Declared natively: yes
- Option source references: 0/0
- Missing option references: none
- Known gaps: Top-level command exists, but several subcommands still use local/offline substitutes rather than real OCI flows.

## `features test`

- Description: Test Features
- Declared natively: yes
- Option source references: 10/13
- Missing option references: `log-level`, `permit-randomization`, `quiet`
- Known gaps: Native test runner exists, but parity with upstream feature resolution and registry-backed dependencies is incomplete.

## `features package`

- Description: Package Features
- Declared natively: yes
- Option source references: 0/0
- Missing option references: none
- Known gaps: Packages local targets, but broader upstream collection behavior is still limited.

## `features publish`

- Description: Package and publish Features
- Declared natively: yes
- Option source references: 0/0
- Missing option references: none
- Known gaps: Publishes a local OCI layout rather than a real authenticated registry push flow.

## `features info`

- Description: Fetch metadata for a published Feature
- Declared natively: yes
- Option source references: 0/2
- Missing option references: `log-level`, `output-format`
- Known gaps: Only `manifest` mode is implemented natively; `tags`, `dependencies`, and `verbose` are missing.

## `features resolve-dependencies`

- Description: Read and resolve dependency graph from a configuration
- Declared natively: yes
- Option source references: 1/2
- Missing option references: `log-level`
- Known gaps: Current implementation follows declared `dependsOn` edges, but still relies on local/manual manifests rather than full OCI graph resolution.

## `features generate-docs`

- Description: Generate documentation
- Declared natively: yes
- Option source references: 1/6
- Missing option references: `github-owner`, `github-repo`, `log-level`, `namespace`, `registry`
- Known gaps: Documentation generation is minimal compared with upstream.

## `templates`

- Description: Templates commands
- Declared natively: yes
- Option source references: 0/0
- Missing option references: none
- Known gaps: Top-level command exists, but published-template flows still rely on embedded/local substitutes.

## `templates apply`

- Description: Apply a template to the project
- Declared natively: yes
- Option source references: 4/7
- Missing option references: `log-level`, `omit-paths`, `tmp-dir`
- Known gaps: Published template application is still based on embedded/local substitutes instead of real OCI fetches.

## `templates publish`

- Description: Package and publish templates
- Declared natively: yes
- Option source references: 0/0
- Missing option references: none
- Known gaps: Publishes a local OCI layout rather than a real authenticated registry push flow.

## `templates metadata`

- Description: Fetch a published Template\'s metadata
- Declared natively: yes
- Option source references: 0/1
- Missing option references: `log-level`
- Known gaps: Published template metadata is still based on embedded/local substitutes instead of real OCI fetches.

## `templates generate-docs`

- Description: Generate documentation
- Declared natively: yes
- Option source references: 1/4
- Missing option references: `github-owner`, `github-repo`, `log-level`
- Known gaps: Documentation generation is minimal compared with upstream.

## `exec`

- Description: Execute a command on a running dev container
- Declared natively: yes
- Option source references: 8/19
- Missing option references: `container-data-folder`, `container-system-data-folder`, `default-user-env-probe`, `log-level`, `mount-git-worktree-common-dir`, `mount-workspace-git-root`, `override-config`, `skip-feature-auto-mapping`, `terminal-columns`, `terminal-rows`, `user-data-folder`
- Known gaps: Core exec path is native, but upstream option coverage is still narrower.
