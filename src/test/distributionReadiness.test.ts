import { expect } from 'chai';

import { evaluateDistributionReadiness } from '../spec-node/migration/distributionReadiness';

describe('distribution readiness evaluator', () => {
	it('marks distribution readiness complete when all productionization checks pass', () => {
		const result = evaluateDistributionReadiness({
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
				commands: ['read-configuration', 'up', 'build', 'exec'],
			},
			releaseDocs: {
				ok: true,
				docPath: 'docs/standalone/distribution.md',
				fallbackInstaller: 'npm i -g @devcontainers/cli',
			},
			experimentalChannel: {
				ok: true,
				artifactSuffix: '-standalone',
				published: true,
			},
		});

		expect(result.complete).to.equal(true);
		expect(result.summary).to.include('Distribution readiness complete');
	});


	it('fails distribution readiness when smoke lane omits required commands', () => {
		const result = evaluateDistributionReadiness({
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
				ok: true,
				lane: 'standalone-smoke',
				commands: ['read-configuration', 'up', 'exec'],
			},
			releaseDocs: {
				ok: true,
				docPath: 'docs/standalone/distribution.md',
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

	it('fails distribution readiness when standalone smoke lane is missing', () => {
		const result = evaluateDistributionReadiness({
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
				docPath: 'docs/standalone/distribution.md',
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
