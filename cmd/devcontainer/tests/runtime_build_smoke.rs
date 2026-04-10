//! Integration test entrypoint for native runtime build smoke suites.

mod support;

#[path = "runtime_build_smoke/compose.rs"]
mod compose;
#[path = "runtime_build_smoke/dockerfile.rs"]
mod dockerfile;
#[path = "runtime_build_smoke/features.rs"]
mod features;
