import { expect } from 'chai';

import { evaluatePhase5, REQUIRED_PHASE5_PARITY_COMMANDS } from '../spec-node/standalonePhase5';

describe('standalone phase 5 evaluator', () => {
	it('marks phase 5 complete when hardening and cutover checks pass', () => {
		const result = evaluatePhase5({
			integrationParity: {
				ok: true,
				baseline: 'node-cli',
				paritySuitePath: 'src/test/native-parity',
				coveredCommands: [...REQUIRED_PHASE5_PARITY_COMMANDS],
			},
			performanceBenchmarks: {
				ok: true,
				reportPath: 'docs/standalone/benchmarks/phase5.md',
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
		expect(result.summary).to.include('Phase 5 complete');
	});

	it('fails phase 5 completion when command parity coverage is incomplete', () => {
		const result = evaluatePhase5({
			integrationParity: {
				ok: true,
				baseline: 'node-cli',
				paritySuitePath: 'src/test/native-parity',
				coveredCommands: ['read-configuration', 'build'],
			},
			performanceBenchmarks: {
				ok: true,
				reportPath: 'docs/standalone/benchmarks/phase5.md',
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
