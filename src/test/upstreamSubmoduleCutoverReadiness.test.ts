import { expect } from 'chai';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'fs';
import os from 'os';
import path from 'path';

import {
	collectRootLevelUpstreamPathReferences,
	collectDuplicateUpstreamPaths,
	evaluateUpstreamSubmoduleCutoverReadiness,
} from '../spec-node/migration/upstreamSubmoduleCutoverReadiness';
import { buildUpstreamPath } from '../spec-node/migration/upstreamPaths';

describe('upstream submodule cutover readiness evaluator', () => {
	it('marks repository layout complete when upstream sources exist only under upstream/', () => {
		const result = evaluateUpstreamSubmoduleCutoverReadiness({
			repositoryLayoutAndOwnership: {
				ok: true,
				upstreamRoot: 'upstream',
				duplicateUpstreamPaths: [],
			},
		});

		expect(result.complete).to.equal(true);
		expect(result.summary).to.include('Upstream submodule cutover readiness complete');
	});

	it('fails when duplicate upstream-owned paths exist outside upstream/', () => {
		const result = evaluateUpstreamSubmoduleCutoverReadiness({
			repositoryLayoutAndOwnership: {
				ok: true,
				upstreamRoot: 'upstream',
				duplicateUpstreamPaths: ['src/spec-node/devContainersSpecCLI.ts'],
			},
		});

		expect(result.complete).to.equal(false);
		expect(result.missingChecks).to.deep.equal(['repository-layout-and-ownership']);
	});
});

describe('collectDuplicateUpstreamPaths', () => {
	it('detects duplicate upstream TypeScript source paths outside upstream/', () => {
		const fixtureRoot = mkdtempSync(path.join(os.tmpdir(), 'upstream-layout-'));
		try {
				mkdirSync(path.join(fixtureRoot, buildUpstreamPath('src', 'spec-node')), { recursive: true });
			mkdirSync(path.join(fixtureRoot, 'src/spec-node'), { recursive: true });
			mkdirSync(path.join(fixtureRoot, 'src/project-owned'), { recursive: true });

			writeFileSync(path.join(fixtureRoot, buildUpstreamPath('src', 'spec-node', 'devContainersSpecCLI.ts')), '// upstream');
			writeFileSync(path.join(fixtureRoot, 'src/spec-node/devContainersSpecCLI.ts'), '// duplicate');
			writeFileSync(path.join(fixtureRoot, 'src/project-owned/nativeOnly.rs'), '// project owned');

			const duplicates = collectDuplicateUpstreamPaths({ repositoryRoot: fixtureRoot });
			expect(duplicates).to.deep.equal(['src/spec-node/devContainersSpecCLI.ts']);
		} finally {
			rmSync(fixtureRoot, { recursive: true, force: true });
		}
	});

	it('returns an empty list when upstream/ is missing', () => {
		const fixtureRoot = mkdtempSync(path.join(os.tmpdir(), 'upstream-layout-'));
		try {
			mkdirSync(path.join(fixtureRoot, 'src/spec-node'), { recursive: true });
			writeFileSync(path.join(fixtureRoot, 'src/spec-node/devContainersSpecCLI.ts'), '// no upstream');

			const duplicates = collectDuplicateUpstreamPaths({ repositoryRoot: fixtureRoot });
			expect(duplicates).to.deep.equal([]);
		} finally {
			rmSync(fixtureRoot, { recursive: true, force: true });
		}
	});

	it('finds no duplicate upstream TypeScript source files in this repository', () => {
		const repositoryRoot = path.resolve(__dirname, '../..');
		const duplicates = collectDuplicateUpstreamPaths({ repositoryRoot });
		expect(duplicates).to.deep.equal([]);
	});
});

describe('collectRootLevelUpstreamPathReferences', () => {
	it('detects root-level upstream path references in build and test command files', () => {
		const fixtureRoot = mkdtempSync(path.join(os.tmpdir(), 'upstream-path-references-'));
		try {
			mkdirSync(path.join(fixtureRoot, buildUpstreamPath('src', 'test')), { recursive: true });
			mkdirSync(path.join(fixtureRoot, 'build'), { recursive: true });

			writeFileSync(path.join(fixtureRoot, buildUpstreamPath('src', 'test', 'cli.test.ts')), '// upstream fixture');
			writeFileSync(path.join(fixtureRoot, 'package.json'), JSON.stringify({
				scripts: {
					test: 'mocha src/test/cli.test.ts',
				},
			}));
			writeFileSync(path.join(fixtureRoot, 'build/check-paths.js'), 'const fixture = "src/test/cli.test.ts";');

			const references = collectRootLevelUpstreamPathReferences({ repositoryRoot: fixtureRoot });
			expect(references.map(reference => `${reference.filePath}:${reference.referencedPath}`)).to.deep.equal([
				'build/check-paths.js:src/test/cli.test.ts',
				'package.json:src/test/cli.test.ts',
			]);
		} finally {
			rmSync(fixtureRoot, { recursive: true, force: true });
		}
	});

	it('ignores references that already use upstream/ prefixes', () => {
		const fixtureRoot = mkdtempSync(path.join(os.tmpdir(), 'upstream-path-references-'));
		try {
			mkdirSync(path.join(fixtureRoot, buildUpstreamPath('src', 'test')), { recursive: true });
			writeFileSync(path.join(fixtureRoot, buildUpstreamPath('src', 'test', 'cli.test.ts')), '// upstream fixture');
			writeFileSync(path.join(fixtureRoot, 'package.json'), JSON.stringify({
				scripts: {
					test: 'mocha upstream/src/test/cli.test.ts',
				},
			}));

			const references = collectRootLevelUpstreamPathReferences({ repositoryRoot: fixtureRoot });
			expect(references).to.deep.equal([]);
		} finally {
			rmSync(fixtureRoot, { recursive: true, force: true });
		}
	});

	it('ignores references when a local path still exists unless explicitly requested', () => {
		const fixtureRoot = mkdtempSync(path.join(os.tmpdir(), 'upstream-path-references-'));
		try {
			mkdirSync(path.join(fixtureRoot, buildUpstreamPath('src', 'test')), { recursive: true });
			mkdirSync(path.join(fixtureRoot, 'src/test'), { recursive: true });
			writeFileSync(path.join(fixtureRoot, buildUpstreamPath('src', 'test', 'cli.test.ts')), '// upstream fixture');
			writeFileSync(path.join(fixtureRoot, 'src/test/cli.test.ts'), '// local fixture');
			writeFileSync(path.join(fixtureRoot, 'package.json'), JSON.stringify({
				scripts: {
					test: 'mocha src/test/cli.test.ts',
				},
			}));

			const references = collectRootLevelUpstreamPathReferences({ repositoryRoot: fixtureRoot });
			expect(references).to.deep.equal([]);

			const referencesIncludingLocal = collectRootLevelUpstreamPathReferences({
				repositoryRoot: fixtureRoot,
				includeExistingLocalPaths: true,
			});
			expect(referencesIncludingLocal.map(reference => `${reference.filePath}:${reference.referencedPath}`)).to.deep.equal([
				'package.json:src/test/cli.test.ts',
			]);
		} finally {
			rmSync(fixtureRoot, { recursive: true, force: true });
		}
	});

	it('uses upstream-prefixed paths for upstream-owned package scripts and fixtures', () => {
		const repositoryRoot = path.resolve(__dirname, '../..');
		const references = collectRootLevelUpstreamPathReferences({
			repositoryRoot,
			scanRoots: ['package.json'],
		});
		expect(references).to.deep.equal([]);
	});
});
