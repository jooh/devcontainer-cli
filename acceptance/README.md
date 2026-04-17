# Acceptance Fixtures

This directory holds repo-owned manual acceptance scenarios for contributor
checks against the native CLI. The suite manifest lives in
`acceptance/scenarios.json`.

Prerequisites:

- a `devcontainer` binary on `PATH`, either from a local build or a release
  artifact
- a working container engine

## image-lifecycle

Path: `acceptance/image-lifecycle`

Use this as the baseline lifecycle workspace.

```bash
devcontainer read-configuration --workspace-folder acceptance/image-lifecycle
devcontainer up --workspace-folder acceptance/image-lifecycle
devcontainer exec --workspace-folder acceptance/image-lifecycle /bin/cat /workspace/.acceptance/post-create
devcontainer run-user-commands --workspace-folder acceptance/image-lifecycle
devcontainer set-up --workspace-folder acceptance/image-lifecycle
```

## dockerfile-build

Path: `acceptance/dockerfile-build`

Use this to verify Dockerfile-based build args and post-create marker flow.

```bash
devcontainer read-configuration --workspace-folder acceptance/dockerfile-build
devcontainer build --workspace-folder acceptance/dockerfile-build --image-name acceptance/dockerfile-build:manual
devcontainer up --workspace-folder acceptance/dockerfile-build
devcontainer exec --workspace-folder acceptance/dockerfile-build /bin/cat /workspace/.acceptance/dockerfile-message
```

## template-node-mongo

Path: `acceptance/template-node-mongo`

This is the template scenario. Apply
`ghcr.io/devcontainers/templates/node-mongo:latest` into the generated
workspace at `acceptance/template-node-mongo/workspace`, then run the normal
runtime checks there. The tracked `workspace/.gitignore` keeps generated files
out of version control.

```bash
devcontainer templates apply --workspace-folder acceptance/template-node-mongo/workspace --template-id ghcr.io/devcontainers/templates/node-mongo:latest
devcontainer read-configuration --workspace-folder acceptance/template-node-mongo/workspace
devcontainer up --workspace-folder acceptance/template-node-mongo/workspace
devcontainer exec --workspace-folder acceptance/template-node-mongo/workspace /bin/sh -lc 'ls /workspace/.devcontainer'
```

## local-feature

Path: `acceptance/local-feature`

Use this to verify repo-local Feature resolution and installation.

```bash
devcontainer read-configuration --workspace-folder acceptance/local-feature
devcontainer build --workspace-folder acceptance/local-feature --image-name acceptance/local-feature:manual
devcontainer up --workspace-folder acceptance/local-feature
devcontainer exec --workspace-folder acceptance/local-feature /usr/local/bin/acceptance-local-feature
```

## published-feature

Path: `acceptance/published-feature`

Use this as the published-Feature scenario. It is the suite's only scenario
that depends on published devcontainer collection resolution.

```bash
devcontainer read-configuration --workspace-folder acceptance/published-feature --include-features-configuration
devcontainer build --workspace-folder acceptance/published-feature --image-name acceptance/published-feature:manual
devcontainer up --workspace-folder acceptance/published-feature
devcontainer exec --workspace-folder acceptance/published-feature /bin/cat /workspace/.acceptance/published-feature
```
