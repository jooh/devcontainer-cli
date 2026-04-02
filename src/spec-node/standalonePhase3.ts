interface CheckResult {
	ok: boolean;
	details?: string;
}

export const REQUIRED_PHASE3_TOP_LEVEL_COMMANDS = [
	'read-configuration',
	'build',
	'up',
	'exec',
	'features',
	'templates',
] as const;

interface RustCrateInput extends CheckResult {
	cratePath: string;
	binaryName: string;
}

interface CliParityInput extends CheckResult {
	topLevelCommands: string[];
	helpParity: boolean;
}

interface LoggingAndExitCodesInput extends CheckResult {
	formats: string[];
	exitCodeParity: boolean;
}

interface CompatibilityBridgeInput extends CheckResult {
	enabled: boolean;
	fallbackCommand: string;
	unportedCommandBehaviorVerified: boolean;
}

interface Phase3Input {
	rustCrate: RustCrateInput;
	cliParity: CliParityInput;
	loggingAndExitCodes: LoggingAndExitCodesInput;
	compatibilityBridge: CompatibilityBridgeInput;
}

interface Phase3Evaluation {
	complete: boolean;
	summary: string;
	missingChecks: Array<'rust-crate' | 'cli-parity' | 'logging-exit-codes' | 'compatibility-bridge'>;
}

function hasRustCrate(input: RustCrateInput) {
	return input.ok
		&& input.cratePath.trim().length > 0
		&& input.binaryName.trim().length > 0;
}

function hasCliParity(input: CliParityInput) {
	const providedCommands = new Set(input.topLevelCommands.map(command => command.trim()).filter(Boolean));
	return input.ok
		&& input.helpParity
		&& REQUIRED_PHASE3_TOP_LEVEL_COMMANDS.every(command => providedCommands.has(command));
}

function hasLoggingAndExitCodeParity(input: LoggingAndExitCodesInput) {
	const formats = new Set(input.formats.map(format => format.trim()).filter(Boolean));
	return input.ok && formats.has('text') && formats.has('json') && input.exitCodeParity;
}

function hasCompatibilityBridge(input: CompatibilityBridgeInput) {
	return input.ok
		&& input.enabled
		&& input.fallbackCommand.trim().length > 0
		&& input.unportedCommandBehaviorVerified;
}

export function evaluatePhase3(input: Phase3Input): Phase3Evaluation {
	const missingChecks: Phase3Evaluation['missingChecks'] = [];

	if (!hasRustCrate(input.rustCrate)) {
		missingChecks.push('rust-crate');
	}
	if (!hasCliParity(input.cliParity)) {
		missingChecks.push('cli-parity');
	}
	if (!hasLoggingAndExitCodeParity(input.loggingAndExitCodes)) {
		missingChecks.push('logging-exit-codes');
	}
	if (!hasCompatibilityBridge(input.compatibilityBridge)) {
		missingChecks.push('compatibility-bridge');
	}

	if (!missingChecks.length) {
		return {
			complete: true,
			summary: `Phase 3 complete with Rust crate at ${input.rustCrate.cratePath}.`,
			missingChecks,
		};
	}

	return {
		complete: false,
		summary: `Phase 3 incomplete. Missing: ${missingChecks.join(', ')}.`,
		missingChecks,
	};
}
