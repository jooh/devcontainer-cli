import { expect } from 'chai';

import { evaluateCutoverReadiness, REQUIRED_CUTOVER_PARITY_COMMANDS } from '../spec-node/migration/cutoverReadiness';

describe('cutover readiness evaluator', () => {
	it('marks cutover readiness complete when hardening and cutover checks pass', () => {
		const result = evaluateCutoverReadiness({
			integrationParity: {
				ok: true,
				baseline: 'node-cli',
				paritySuitePath: 'src/test/native-parity',
				coveredCommands: [...REQUIRED_CUTOVER_PARITY_COMMANDS],
			},
			performanceBenchmarks: {
				ok: true,
				reportPath: 'docs/standalone/cutover.md',
				startupLatencyMs: 220,
				peakMemoryMb: 96,
			},
			defaultReleaseCutover: {
				ok: true,
				nativeDefault: true,
				nodeFallbackWindow: '1 major release',
			},
			fallbackRemoval: {
				ok: true,
				criteria: 'No Sev1 parity regressions for 2 releases',
				removalIssue: 'https://example.test/issues/123',
				planned: true,
			},
		});

		expect(result.complete).to.equal(true);
		expect(result.summary).to.include('Cutover readiness complete');
	});

	it('fails cutover readiness when command parity coverage is incomplete', () => {
		const result = evaluateCutoverReadiness({
			integrationParity: {
				ok: true,
				baseline: 'node-cli',
				paritySuitePath: 'src/test/native-parity',
				coveredCommands: ['read-configuration', 'build'],
			},
			performanceBenchmarks: {
				ok: true,
				reportPath: 'docs/standalone/cutover.md',
				startupLatencyMs: 220,
				peakMemoryMb: 96,
			},
			defaultReleaseCutover: {
				ok: true,
				nativeDefault: true,
				nodeFallbackWindow: '1 major release',
			},
			fallbackRemoval: {
				ok: true,
				criteria: 'No Sev1 parity regressions for 2 releases',
				removalIssue: 'https://example.test/issues/123',
				planned: true,
			},
		});

		expect(result.complete).to.equal(false);
		expect(result.missingChecks).to.deep.equal(['integration-parity']);
	});
});
