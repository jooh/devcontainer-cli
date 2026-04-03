import { existsSync, readdirSync, statSync } from 'fs';
import path from 'path';

interface CheckResult {
	ok: boolean;
	details?: string;
}

interface RepositoryLayoutAndOwnershipInput extends CheckResult {
	upstreamRoot: string;
	duplicateUpstreamPaths: string[];
}

interface UpstreamSubmoduleCutoverReadinessInput {
	repositoryLayoutAndOwnership: RepositoryLayoutAndOwnershipInput;
}

interface UpstreamSubmoduleCutoverReadinessEvaluation {
	complete: boolean;
	summary: string;
	missingChecks: Array<'repository-layout-and-ownership'>;
}

interface DuplicatePathScanOptions {
	repositoryRoot: string;
	upstreamRoot?: string;
	sourceRoots?: string[];
	includedExtensions?: string[];
}

const DEFAULT_SOURCE_ROOTS = ['src'];
const DEFAULT_INCLUDED_EXTENSIONS = new Set(['.ts', '.tsx']);

function walkRelativeFiles(rootDir: string, startDir = rootDir): string[] {
	const entries = readdirSync(startDir, { withFileTypes: true });
	const files: string[] = [];

	for (const entry of entries) {
		const absolutePath = path.join(startDir, entry.name);
		const relativePath = path.relative(rootDir, absolutePath).split(path.sep).join('/');
		if (entry.isDirectory()) {
			files.push(...walkRelativeFiles(rootDir, absolutePath));
			continue;
		}
		if (entry.isFile()) {
			files.push(relativePath);
		}
	}

	return files;
}

function isIncludedSourceFile(relativePath: string, extensions: Set<string>) {
	return extensions.has(path.extname(relativePath));
}

export function collectDuplicateUpstreamPaths(options: DuplicatePathScanOptions) {
	const upstreamRoot = options.upstreamRoot ?? 'upstream';
	const sourceRoots = options.sourceRoots ?? DEFAULT_SOURCE_ROOTS;
	const includedExtensions = new Set(options.includedExtensions ?? [...DEFAULT_INCLUDED_EXTENSIONS]);
	const upstreamAbsoluteRoot = path.join(options.repositoryRoot, upstreamRoot);

	if (!existsSync(upstreamAbsoluteRoot) || !statSync(upstreamAbsoluteRoot).isDirectory()) {
		return [];
	}

	const duplicatePaths: string[] = [];

	for (const sourceRoot of sourceRoots) {
		const upstreamSourceRoot = path.join(upstreamAbsoluteRoot, sourceRoot);
		const localSourceRoot = path.join(options.repositoryRoot, sourceRoot);
		if (!existsSync(upstreamSourceRoot) || !existsSync(localSourceRoot)) {
			continue;
		}

		const upstreamFiles = new Set(
			walkRelativeFiles(upstreamSourceRoot)
				.filter(relativePath => isIncludedSourceFile(relativePath, includedExtensions)),
		);
		if (!upstreamFiles.size) {
			continue;
		}

		for (const localRelativePath of walkRelativeFiles(localSourceRoot)) {
			if (!isIncludedSourceFile(localRelativePath, includedExtensions)) {
				continue;
			}
			if (upstreamFiles.has(localRelativePath)) {
				duplicatePaths.push(`${sourceRoot}/${localRelativePath}`);
			}
		}
	}

	return duplicatePaths.sort((a, b) => a.localeCompare(b));
}

function hasCanonicalUpstreamLocation(input: RepositoryLayoutAndOwnershipInput) {
	return input.ok
		&& input.upstreamRoot.trim().length > 0
		&& input.duplicateUpstreamPaths.length === 0;
}

export function evaluateUpstreamSubmoduleCutoverReadiness(input: UpstreamSubmoduleCutoverReadinessInput): UpstreamSubmoduleCutoverReadinessEvaluation {
	const missingChecks: UpstreamSubmoduleCutoverReadinessEvaluation['missingChecks'] = [];

	if (!hasCanonicalUpstreamLocation(input.repositoryLayoutAndOwnership)) {
		missingChecks.push('repository-layout-and-ownership');
	}

	if (!missingChecks.length) {
		return {
			complete: true,
			summary: `Upstream submodule cutover readiness complete with canonical upstream root at ${input.repositoryLayoutAndOwnership.upstreamRoot}.`,
			missingChecks,
		};
	}

	return {
		complete: false,
		summary: `Upstream submodule cutover readiness incomplete. Missing: ${missingChecks.join(', ')}.`,
		missingChecks,
	};
}
