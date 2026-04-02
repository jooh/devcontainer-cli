interface CheckResult {
	ok: boolean;
	details?: string;
}

interface ReproducibleBuildInput extends CheckResult {
	workflowPath: string;
	deterministicInputs: string[];
}

interface SigningInput extends CheckResult {
	strategy: string;
}

interface PackagedSmokeTestsInput extends CheckResult {
	lane: string;
	commands: string[];
}

interface ReleaseDocsInput extends CheckResult {
	docPath: string;
	fallbackInstaller: string;
}

interface ExperimentalChannelInput extends CheckResult {
	artifactSuffix: string;
	published: boolean;
}

interface DistributionReadinessInput {
	reproducibleBuild: ReproducibleBuildInput;
	signing: SigningInput;
	packagedSmokeTests: PackagedSmokeTestsInput;
	releaseDocs: ReleaseDocsInput;
	experimentalChannel: ExperimentalChannelInput;
}

interface DistributionReadinessEvaluation {
	complete: boolean;
	summary: string;
	missingChecks: Array<
		| 'reproducible-build'
		| 'signing'
		| 'packaged-smoke-tests'
		| 'release-docs'
		| 'experimental-channel'>;
}

function hasReproducibleBuild(input: ReproducibleBuildInput) {
	return input.ok
		&& input.workflowPath.trim().length > 0
		&& input.deterministicInputs.length > 0;
}

function hasSigningStrategy(input: SigningInput) {
	return input.ok && input.strategy.trim().length > 0;
}

function hasPackagedSmokeLane(input: PackagedSmokeTestsInput) {
	const requiredCommands = ['read-configuration', 'up', 'build', 'exec'];
	const providedCommands = new Set(input.commands.map(command => command.trim()).filter(Boolean));

	return input.ok
		&& input.lane.trim().length > 0
		&& requiredCommands.every(command => providedCommands.has(command));
}

function hasReleaseDocs(input: ReleaseDocsInput) {
	return input.ok && input.docPath.trim().length > 0 && input.fallbackInstaller.trim().length > 0;
}

function hasExperimentalChannel(input: ExperimentalChannelInput) {
	return input.ok && input.artifactSuffix.trim().length > 0 && input.published;
}

export function evaluateDistributionReadiness(input: DistributionReadinessInput): DistributionReadinessEvaluation {
	const missingChecks: DistributionReadinessEvaluation['missingChecks'] = [];

	if (!hasReproducibleBuild(input.reproducibleBuild)) {
		missingChecks.push('reproducible-build');
	}
	if (!hasSigningStrategy(input.signing)) {
		missingChecks.push('signing');
	}
	if (!hasPackagedSmokeLane(input.packagedSmokeTests)) {
		missingChecks.push('packaged-smoke-tests');
	}
	if (!hasReleaseDocs(input.releaseDocs)) {
		missingChecks.push('release-docs');
	}
	if (!hasExperimentalChannel(input.experimentalChannel)) {
		missingChecks.push('experimental-channel');
	}

	if (!missingChecks.length) {
		return {
			complete: true,
			summary: `Distribution readiness complete with reproducible builds in ${input.reproducibleBuild.workflowPath}.`,
			missingChecks,
		};
	}

	return {
		complete: false,
		summary: `Distribution readiness incomplete. Missing: ${missingChecks.join(', ')}.`,
		missingChecks,
	};
}
