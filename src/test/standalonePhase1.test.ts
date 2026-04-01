import { expect } from 'chai';

import { evaluatePhase1, REQUIRED_PHASE1_COMMANDS } from '../spec-node/standalonePhase1';

describe('standalone phase 1 evaluator', () => {
	it('marks phase 1 complete when all required checks pass', () => {
		const result = evaluatePhase1({
			prototype: { strategy: 'node-sea', binaryPath: 'dist/devcontainer-linux-x64' },
			commandCoverage: Object.fromEntries(REQUIRED_PHASE1_COMMANDS.map(command => [command, { ok: true }])),
			composeValidation: { ok: true },
			blockers: [
				{ id: 'node-pty-sea', severity: 'high', mitigation: 'Extract native modules next to SEA binary.' },
			],
			benchmarks: {
				standaloneSizeBytes: 72 * 1024 * 1024,
				baselineSizeBytes: 89 * 1024 * 1024,
				standaloneHelpColdStartMs: 210,
				baselineHelpColdStartMs: 285,
			},
		});

		expect(result.complete).to.equal(true);
		expect(result.summary).to.include('Phase 1 complete');
	});

	it('fails phase 1 completion when command coverage is partial', () => {
		const [firstCommand] = REQUIRED_PHASE1_COMMANDS;
		const result = evaluatePhase1({
			prototype: { strategy: 'node-sea', binaryPath: 'dist/devcontainer-linux-x64' },
			commandCoverage: {
				[firstCommand]: { ok: false, details: 'binary failed with exit code 1' },
			},
			composeValidation: { ok: true },
			blockers: [{ id: 'dynamic-require-audit', severity: 'medium', mitigation: 'Bundle dynamic imports.' }],
			benchmarks: {
				standaloneSizeBytes: 72,
				baselineSizeBytes: 89,
				standaloneHelpColdStartMs: 210,
				baselineHelpColdStartMs: 285,
			},
		});

		expect(result.complete).to.equal(false);
		expect(result.missingChecks).to.deep.equal(['command-coverage']);
	});
});
