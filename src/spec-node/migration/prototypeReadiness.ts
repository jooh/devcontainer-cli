export const REQUIRED_PROTOTYPE_COMMANDS = [
	'up',
	'build',
	'exec',
	'read-configuration',
	'features',
	'templates',
] as const;

export type PrototypeCommand = (typeof REQUIRED_PROTOTYPE_COMMANDS)[number];

interface CheckResult {
	ok: boolean;
	details?: string;
}

interface PrototypeReadinessInput {
	prototype: {
		strategy: 'node-sea' | 'pkg' | 'nexe' | 'other';
		binaryPath: string;
	};
	commandCoverage: Partial<Record<PrototypeCommand, CheckResult>>;
	composeValidation: CheckResult;
	blockers: Array<{ id: string; severity: 'low' | 'medium' | 'high'; mitigation: string }>;
	benchmarks: {
		standaloneSizeBytes: number;
		baselineSizeBytes: number;
		standaloneHelpColdStartMs: number;
		baselineHelpColdStartMs: number;
	};
}

interface PrototypeReadinessEvaluation {
	complete: boolean;
	summary: string;
	missingChecks: Array<'prototype' | 'command-coverage' | 'compose-validation' | 'blockers' | 'benchmarks'>;
}

function hasCommandCoverage(commandCoverage: PrototypeReadinessInput['commandCoverage']) {
	return REQUIRED_PROTOTYPE_COMMANDS.every(command => commandCoverage[command]?.ok === true);
}

function hasBenchmarkComparison(benchmarks: PrototypeReadinessInput['benchmarks']) {
	return Number.isFinite(benchmarks.standaloneSizeBytes)
		&& Number.isFinite(benchmarks.baselineSizeBytes)
		&& Number.isFinite(benchmarks.standaloneHelpColdStartMs)
		&& Number.isFinite(benchmarks.baselineHelpColdStartMs)
		&& benchmarks.standaloneSizeBytes > 0
		&& benchmarks.baselineSizeBytes > 0
		&& benchmarks.standaloneHelpColdStartMs > 0
		&& benchmarks.baselineHelpColdStartMs > 0;
}

export function evaluatePrototypeReadiness(input: PrototypeReadinessInput): PrototypeReadinessEvaluation {
	const missingChecks: PrototypeReadinessEvaluation['missingChecks'] = [];

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
			summary: `Prototype readiness complete via ${input.prototype.strategy} prototype at ${input.prototype.binaryPath}.`,
			missingChecks,
		};
	}

	return {
		complete: false,
		summary: `Prototype readiness incomplete. Missing: ${missingChecks.join(', ')}.`,
		missingChecks,
	};
}
