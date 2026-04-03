import { execFileSync } from 'child_process';

interface ResolvePinnedUpstreamCommitOptions {
	repositoryRoot: string;
	submodulePath?: string;
	runGit?: (cwd: string, args: string[]) => string;
}

function defaultRunGit(cwd: string, args: string[]) {
	return execFileSync('git', args, {
		cwd,
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe'],
	});
}

export function resolvePinnedUpstreamCommit(options: ResolvePinnedUpstreamCommitOptions) {
	const submodulePath = options.submodulePath ?? 'upstream';
	const runGit = options.runGit ?? defaultRunGit;
	const output = runGit(options.repositoryRoot, ['rev-parse', `HEAD:${submodulePath}`]).trim();

	if (!output) {
		throw new Error(`Unable to resolve pinned upstream commit for ${submodulePath}.`);
	}

	return output;
}

export function formatUpstreamCompatibilityContract(commit: string) {
	return `This repository targets upstream/ at commit ${commit}.`;
}

export function formatUpstreamCommitTraceLine(commit: string) {
	return `[upstream-compat] pinned upstream commit: ${commit}`;
}

interface UpstreamCommitRegressionInput {
	recordedCommit: string;
	currentCommit: string;
}

interface UpstreamCommitRegressionReport {
	hasRegression: boolean;
	summary: string;
}

export function reportUpstreamCommitRegression(input: UpstreamCommitRegressionInput): UpstreamCommitRegressionReport {
	if (input.recordedCommit === input.currentCommit) {
		return {
			hasRegression: false,
			summary: `Pinned upstream commit unchanged at ${input.currentCommit}.`,
		};
	}

	return {
		hasRegression: true,
		summary: `Pinned upstream commit changed from ${input.recordedCommit} to ${input.currentCommit}.`,
	};
}
