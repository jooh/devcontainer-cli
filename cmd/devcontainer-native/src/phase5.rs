use std::collections::HashSet;

pub const REQUIRED_PHASE5_PARITY_COMMANDS: [&str; 6] = [
    "read-configuration",
    "build",
    "up",
    "exec",
    "features",
    "templates",
];

pub struct IntegrationParityInput {
    pub ok: bool,
    pub baseline: String,
    pub parity_suite_path: String,
    pub covered_commands: Vec<String>,
}

pub struct PerformanceBenchmarksInput {
    pub ok: bool,
    pub report_path: String,
    pub startup_latency_ms: u64,
    pub peak_memory_mb: u64,
}

pub struct DefaultReleaseCutoverInput {
    pub ok: bool,
    pub native_default: bool,
    pub node_fallback_window: String,
}

pub struct FallbackRemovalInput {
    pub ok: bool,
    pub criteria: String,
    pub removal_issue: String,
    pub planned: bool,
}

pub struct Phase5Input {
    pub integration_parity: IntegrationParityInput,
    pub performance_benchmarks: PerformanceBenchmarksInput,
    pub default_release_cutover: DefaultReleaseCutoverInput,
    pub fallback_removal: FallbackRemovalInput,
}

#[derive(Debug, PartialEq)]
pub enum Phase5MissingCheck {
    IntegrationParity,
    PerformanceBenchmarks,
    DefaultReleaseCutover,
    FallbackRemoval,
}

#[derive(Debug, PartialEq)]
pub struct Phase5Evaluation {
    pub complete: bool,
    pub summary: String,
    pub missing_checks: Vec<Phase5MissingCheck>,
}

fn has_integration_parity(input: &IntegrationParityInput) -> bool {
    let commands: HashSet<&str> = input
        .covered_commands
        .iter()
        .map(|command| command.trim())
        .filter(|command| !command.is_empty())
        .collect();

    input.ok
        && !input.baseline.trim().is_empty()
        && !input.parity_suite_path.trim().is_empty()
        && REQUIRED_PHASE5_PARITY_COMMANDS
            .iter()
            .all(|required| commands.contains(required))
}

fn has_performance_benchmarks(input: &PerformanceBenchmarksInput) -> bool {
    input.ok
        && !input.report_path.trim().is_empty()
        && input.startup_latency_ms > 0
        && input.peak_memory_mb > 0
}

fn has_default_release_cutover(input: &DefaultReleaseCutoverInput) -> bool {
    input.ok && input.native_default && !input.node_fallback_window.trim().is_empty()
}

fn has_fallback_removal(input: &FallbackRemovalInput) -> bool {
    input.ok && !input.criteria.trim().is_empty() && !input.removal_issue.trim().is_empty() && input.planned
}

pub fn evaluate_phase5(input: &Phase5Input) -> Phase5Evaluation {
    let mut missing_checks = Vec::new();

    if !has_integration_parity(&input.integration_parity) {
        missing_checks.push(Phase5MissingCheck::IntegrationParity);
    }

    if !has_performance_benchmarks(&input.performance_benchmarks) {
        missing_checks.push(Phase5MissingCheck::PerformanceBenchmarks);
    }

    if !has_default_release_cutover(&input.default_release_cutover) {
        missing_checks.push(Phase5MissingCheck::DefaultReleaseCutover);
    }

    if !has_fallback_removal(&input.fallback_removal) {
        missing_checks.push(Phase5MissingCheck::FallbackRemoval);
    }

    if missing_checks.is_empty() {
        return Phase5Evaluation {
            complete: true,
            summary: format!(
                "Phase 5 complete with parity suite at {}.",
                input.integration_parity.parity_suite_path
            ),
            missing_checks,
        };
    }

    let missing_labels = missing_checks
        .iter()
        .map(|missing_check| match missing_check {
            Phase5MissingCheck::IntegrationParity => "integration-parity",
            Phase5MissingCheck::PerformanceBenchmarks => "performance-benchmarks",
            Phase5MissingCheck::DefaultReleaseCutover => "default-release-cutover",
            Phase5MissingCheck::FallbackRemoval => "fallback-removal",
        })
        .collect::<Vec<_>>()
        .join(", ");

    Phase5Evaluation {
        complete: false,
        summary: format!("Phase 5 incomplete. Missing: {missing_labels}."),
        missing_checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_input() -> Phase5Input {
        Phase5Input {
            integration_parity: IntegrationParityInput {
                ok: true,
                baseline: "node-cli".to_string(),
                parity_suite_path: "src/test/native-parity".to_string(),
                covered_commands: REQUIRED_PHASE5_PARITY_COMMANDS
                    .iter()
                    .map(|command| (*command).to_string())
                    .collect(),
            },
            performance_benchmarks: PerformanceBenchmarksInput {
                ok: true,
                report_path: "docs/standalone/phase5.md".to_string(),
                startup_latency_ms: 220,
                peak_memory_mb: 96,
            },
            default_release_cutover: DefaultReleaseCutoverInput {
                ok: true,
                native_default: true,
                node_fallback_window: "1 major cycle".to_string(),
            },
            fallback_removal: FallbackRemovalInput {
                ok: true,
                criteria: "No Sev1 regressions across two releases".to_string(),
                removal_issue: "https://example.test/issues/123".to_string(),
                planned: true,
            },
        }
    }

    #[test]
    fn marks_phase5_complete_when_all_checks_pass() {
        let input = complete_input();
        let result = evaluate_phase5(&input);

        assert!(result.complete);
        assert!(result.summary.contains("Phase 5 complete"));
        assert!(result.missing_checks.is_empty());
    }

    #[test]
    fn fails_phase5_when_parity_coverage_is_incomplete() {
        let mut input = complete_input();
        input.integration_parity.covered_commands = vec!["read-configuration".to_string(), "build".to_string()];

        let result = evaluate_phase5(&input);

        assert!(!result.complete);
        assert_eq!(
            result.missing_checks,
            vec![Phase5MissingCheck::IntegrationParity]
        );
    }
}
