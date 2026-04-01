import { expect } from 'chai';

import { evaluatePhase2 } from '../spec-node/standalonePhase2';

describe('standalone phase 2 evaluator', () => {
	it('marks phase 2 complete when all productionization checks pass', () => {
		const result = evaluatePhase2({
			reproducibleBuild: {
				ok: true,
				workflowPath: '.github/workflows/standalone-release.yml',
				deterministicInputs: ['node-version=20.19.1', 'esbuild=0.27.3', 'SOURCE_DATE_EPOCH'],
			},
			signing: {
				ok: true,
				strategy: 'cosign keyless signatures for Linux artifacts; notarization deferred to non-Linux targets.',
			},
			packagedSmokeTests: {
				ok: true,
				lane: 'standalone-smoke',
				commands: ['read-configuration', 'up', 'exec'],
			},
			releaseDocs: {
				ok: true,
				docPath: 'docs/standalone/phase2.md',
				fallbackInstaller: 'npm i -g @devcontainers/cli',
			},
			experimentalChannel: {
				ok: true,
				artifactSuffix: '-standalone',
				published: true,
			},
		});

		expect(result.complete).to.equal(true);
		expect(result.summary).to.include('Phase 2 complete');
	});

	it('fails phase 2 completion when standalone smoke lane is missing', () => {
		const result = evaluatePhase2({
			reproducibleBuild: {
				ok: true,
				workflowPath: '.github/workflows/standalone-release.yml',
				deterministicInputs: ['node-version=20.19.1'],
			},
			signing: {
				ok: true,
				strategy: 'cosign keyless signatures',
			},
			packagedSmokeTests: {
				ok: false,
				lane: '',
				commands: [],
			},
			releaseDocs: {
				ok: true,
				docPath: 'docs/standalone/phase2.md',
				fallbackInstaller: 'npm i -g @devcontainers/cli',
			},
			experimentalChannel: {
				ok: true,
				artifactSuffix: '-standalone',
				published: true,
			},
		});

		expect(result.complete).to.equal(false);
		expect(result.missingChecks).to.deep.equal(['packaged-smoke-tests']);
	});
});
