export const REQUIRED_PHASE1_COMMANDS = [
	'up',
	'build',
	'exec',
	'read-configuration',
	'features',
	'templates',
] as const;

export type Phase1Command = (typeof REQUIRED_PHASE1_COMMANDS)[number];

interface CheckResult {
	ok: boolean;
	details?: string;
}

interface Phase1Input {
	prototype: {
		strategy: 'node-sea' | 'pkg' | 'nexe' | 'other';
		binaryPath: string;
	};
	commandCoverage: Partial<Record<Phase1Command, CheckResult>>;
	composeValidation: CheckResult;
	blockers: Array<{ id: string; severity: 'low' | 'medium' | 'high'; mitigation: string }>;
	benchmarks: {
		standaloneSizeBytes: number;
		baselineSizeBytes: number;
		standaloneHelpColdStartMs: number;
		baselineHelpColdStartMs: number;
	};
}

interface Phase1Evaluation {
	complete: boolean;
	summary: string;
	missingChecks: Array<'prototype' | 'command-coverage' | 'compose-validation' | 'blockers' | 'benchmarks'>;
}

function hasCommandCoverage(commandCoverage: Phase1Input['commandCoverage']) {
	return REQUIRED_PHASE1_COMMANDS.every(command => commandCoverage[command]?.ok === true);
}

function hasBenchmarkComparison(benchmarks: Phase1Input['benchmarks']) {
	return Number.isFinite(benchmarks.standaloneSizeBytes)
		&& Number.isFinite(benchmarks.baselineSizeBytes)
		&& Number.isFinite(benchmarks.standaloneHelpColdStartMs)
		&& Number.isFinite(benchmarks.baselineHelpColdStartMs)
		&& benchmarks.standaloneSizeBytes > 0
		&& benchmarks.baselineSizeBytes > 0
		&& benchmarks.standaloneHelpColdStartMs > 0
		&& benchmarks.baselineHelpColdStartMs > 0;
}

export function evaluatePhase1(input: Phase1Input): Phase1Evaluation {
	const missingChecks: Phase1Evaluation['missingChecks'] = [];

	if (!input.prototype.binaryPath.trim()) {
		missingChecks.push('prototype');
	}
	if (!hasCommandCoverage(input.commandCoverage)) {
		missingChecks.push('command-coverage');
	}
	if (!input.composeValidation.ok) {
		missingChecks.push('compose-validation');
	}
	if (!input.blockers.length) {
		missingChecks.push('blockers');
	}
	if (!hasBenchmarkComparison(input.benchmarks)) {
		missingChecks.push('benchmarks');
	}

	if (!missingChecks.length) {
		return {
			complete: true,
			summary: `Phase 1 complete via ${input.prototype.strategy} prototype at ${input.prototype.binaryPath}.`,
			missingChecks,
		};
	}

	return {
		complete: false,
		summary: `Phase 1 incomplete. Missing: ${missingChecks.join(', ')}.`,
		missingChecks,
	};
}
