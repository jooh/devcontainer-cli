//! Integration test entrypoint for native runtime container smoke suites.

mod support;

#[path = "runtime_container_smoke/basic.rs"]
mod basic;
#[path = "runtime_container_smoke/compose_flow.rs"]
mod compose_flow;
#[path = "runtime_container_smoke/compose_project.rs"]
mod compose_project;
#[path = "runtime_container_smoke/reuse.rs"]
mod reuse;
