import { existsSync, readdirSync, readFileSync, statSync } from 'fs';
import path from 'path';
import { DEFAULT_UPSTREAM_SUBMODULE_ROOT } from './upstreamPaths';

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

interface RootLevelPathReference {
	filePath: string;
	lineNumber: number;
	referencedPath: string;
}

interface RootLevelPathReferenceScanOptions {
	repositoryRoot: string;
	upstreamRoot?: string;
	scanRoots?: string[];
	fileExtensions?: string[];
	includeExistingLocalPaths?: boolean;
}

const DEFAULT_SOURCE_ROOTS = ['src'];
const DEFAULT_INCLUDED_EXTENSIONS = new Set(['.ts', '.tsx']);
const DEFAULT_REFERENCE_SCAN_ROOTS = ['package.json', 'esbuild.js', 'build', 'scripts', 'src/test'];
const DEFAULT_REFERENCE_SCAN_EXTENSIONS = new Set(['.ts', '.js', '.json', '.md', '.sh', '.yml', '.yaml']);

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

function scanFilesForPathReferences(options: RootLevelPathReferenceScanOptions) {
	const scanRoots = options.scanRoots ?? DEFAULT_REFERENCE_SCAN_ROOTS;
	const fileExtensions = new Set(options.fileExtensions ?? [...DEFAULT_REFERENCE_SCAN_EXTENSIONS]);
	const filesToScan: string[] = [];

	for (const scanRoot of scanRoots) {
		const scanAbsolutePath = path.join(options.repositoryRoot, scanRoot);
		if (!existsSync(scanAbsolutePath)) {
			continue;
		}

		if (statSync(scanAbsolutePath).isDirectory()) {
			for (const relativePath of walkRelativeFiles(scanAbsolutePath)) {
				if (fileExtensions.has(path.extname(relativePath))) {
					filesToScan.push(path.join(scanRoot, relativePath).split(path.sep).join('/'));
				}
			}
			continue;
		}

		if (fileExtensions.has(path.extname(scanRoot)) || path.basename(scanRoot) === 'package.json') {
			filesToScan.push(scanRoot.split(path.sep).join('/'));
		}
	}

	return filesToScan.sort((a, b) => a.localeCompare(b));
}

function normalizeCandidateReference(rawCandidate: string) {
	return rawCandidate
		.replace(/^[("'`]+/, '')
		.replace(/[)"'`,;:.]+$/, '')
		.replace(/^\.\//, '');
}

function collectLinePathCandidates(line: string) {
	const candidateRegex = /(?:\.\/)?(?:[A-Za-z0-9_.-]+\/)+[A-Za-z0-9_.-]+/g;
	const matches = line.match(candidateRegex);
	if (!matches) {
		return [];
	}

	return [...new Set(matches.map(candidate => normalizeCandidateReference(candidate)))];
}

export function collectDuplicateUpstreamPaths(options: DuplicatePathScanOptions) {
	const upstreamRoot = options.upstreamRoot ?? DEFAULT_UPSTREAM_SUBMODULE_ROOT;
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

export function collectRootLevelUpstreamPathReferences(options: RootLevelPathReferenceScanOptions): RootLevelPathReference[] {
	const upstreamRoot = options.upstreamRoot ?? DEFAULT_UPSTREAM_SUBMODULE_ROOT;
	const references: RootLevelPathReference[] = [];
	const filesToScan = scanFilesForPathReferences(options);

	for (const filePath of filesToScan) {
		const absoluteFilePath = path.join(options.repositoryRoot, filePath);
		const content = readFileSync(absoluteFilePath, 'utf8');
		const lines = content.split(/\r?\n/);

		for (let index = 0; index < lines.length; index += 1) {
			const line = lines[index];
			for (const candidate of collectLinePathCandidates(line)) {
				if (!candidate || candidate.startsWith(`${upstreamRoot}/`)) {
					continue;
				}
				if (!candidate.includes('/')) {
					continue;
				}

				const upstreamCandidatePath = path.join(options.repositoryRoot, upstreamRoot, candidate);
				if (!existsSync(upstreamCandidatePath)) {
					continue;
				}
				const localCandidatePath = path.join(options.repositoryRoot, candidate);
				if (!options.includeExistingLocalPaths && existsSync(localCandidatePath)) {
					continue;
				}

				references.push({
					filePath,
					lineNumber: index + 1,
					referencedPath: candidate,
				});
			}
		}
	}

	return references.sort((a, b) => {
		if (a.filePath !== b.filePath) {
			return a.filePath.localeCompare(b.filePath);
		}
		if (a.lineNumber !== b.lineNumber) {
			return a.lineNumber - b.lineNumber;
		}
		return a.referencedPath.localeCompare(b.referencedPath);
	});
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
