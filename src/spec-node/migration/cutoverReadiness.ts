interface CheckResult {
	ok: boolean;
	details?: string;
}

export const REQUIRED_CUTOVER_PARITY_COMMANDS = [
	'read-configuration',
	'build',
	'up',
	'set-up',
	'run-user-commands',
	'outdated',
	'upgrade',
	'exec',
	'features',
	'templates',
] as const;

interface IntegrationParityInput extends CheckResult {
	baseline: string;
	paritySuitePath: string;
	coveredCommands: string[];
}

interface PerformanceBenchmarkInput extends CheckResult {
	reportPath: string;
	startupLatencyMs: number;
	peakMemoryMb: number;
}

interface DefaultReleaseCutoverInput extends CheckResult {
	nativeDefault: boolean;
	nodeFallbackWindow: string;
}

interface FallbackRemovalInput extends CheckResult {
	criteria: string;
	removalIssue: string;
	planned: boolean;
}

interface CutoverReadinessInput {
	integrationParity: IntegrationParityInput;
	performanceBenchmarks: PerformanceBenchmarkInput;
	defaultReleaseCutover: DefaultReleaseCutoverInput;
	fallbackRemoval: FallbackRemovalInput;
}

interface CutoverReadinessEvaluation {
	complete: boolean;
	summary: string;
	missingChecks: Array<
		'integration-parity'
		| 'performance-benchmarks'
		| 'default-release-cutover'
		| 'fallback-removal'>;
}

function hasIntegrationParity(input: IntegrationParityInput) {
	const commands = new Set(input.coveredCommands.map(command => command.trim()).filter(Boolean));
	return input.ok
		&& input.baseline.trim().length > 0
		&& input.paritySuitePath.trim().length > 0
		&& REQUIRED_CUTOVER_PARITY_COMMANDS.every(command => commands.has(command));
}

function hasPerformanceBenchmarks(input: PerformanceBenchmarkInput) {
	return input.ok
		&& input.reportPath.trim().length > 0
		&& input.startupLatencyMs > 0
		&& input.peakMemoryMb > 0;
}

function hasDefaultReleaseCutover(input: DefaultReleaseCutoverInput) {
	return input.ok
		&& input.nativeDefault
		&& input.nodeFallbackWindow.trim().length > 0;
}

function hasFallbackRemovalPlan(input: FallbackRemovalInput) {
	return input.ok
		&& input.criteria.trim().length > 0
		&& input.removalIssue.trim().length > 0
		&& input.planned;
}

export function evaluateCutoverReadiness(input: CutoverReadinessInput): CutoverReadinessEvaluation {
	const missingChecks: CutoverReadinessEvaluation['missingChecks'] = [];

	if (!hasIntegrationParity(input.integrationParity)) {
		missingChecks.push('integration-parity');
	}
	if (!hasPerformanceBenchmarks(input.performanceBenchmarks)) {
		missingChecks.push('performance-benchmarks');
	}
	if (!hasDefaultReleaseCutover(input.defaultReleaseCutover)) {
		missingChecks.push('default-release-cutover');
	}
	if (!hasFallbackRemovalPlan(input.fallbackRemoval)) {
		missingChecks.push('fallback-removal');
	}

	if (!missingChecks.length) {
		return {
			complete: true,
			summary: `Cutover readiness complete with parity suite at ${input.integrationParity.paritySuitePath}.`,
			missingChecks,
		};
	}

	return {
		complete: false,
		summary: `Cutover readiness incomplete. Missing: ${missingChecks.join(', ')}.`,
		missingChecks,
	};
}
