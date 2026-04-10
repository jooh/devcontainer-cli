//! Shared helpers for configuration command unit tests.

use std::path::PathBuf;

pub(super) fn unique_temp_dir() -> PathBuf {
    crate::test_support::unique_temp_dir("devcontainer-config-command-test")
}
