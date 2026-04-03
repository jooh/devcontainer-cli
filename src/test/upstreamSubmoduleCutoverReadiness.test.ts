import { expect } from 'chai';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'fs';
import os from 'os';
import path from 'path';

import {
	collectDuplicateUpstreamPaths,
	evaluateUpstreamSubmoduleCutoverReadiness,
} from '../spec-node/migration/upstreamSubmoduleCutoverReadiness';

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
			mkdirSync(path.join(fixtureRoot, 'upstream/src/spec-node'), { recursive: true });
			mkdirSync(path.join(fixtureRoot, 'src/spec-node'), { recursive: true });
			mkdirSync(path.join(fixtureRoot, 'src/project-owned'), { recursive: true });

			writeFileSync(path.join(fixtureRoot, 'upstream/src/spec-node/devContainersSpecCLI.ts'), '// upstream');
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
