//! Integration test entrypoint for runtime lifecycle smoke suites.

mod support;

#[path = "runtime_lifecycle_smoke/commands.rs"]
mod commands;
#[path = "runtime_lifecycle_smoke/dotfiles.rs"]
mod dotfiles;
#[path = "runtime_lifecycle_smoke/end_to_end.rs"]
mod end_to_end;
#[path = "runtime_lifecycle_smoke/selection.rs"]
mod selection;
