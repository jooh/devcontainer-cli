import { expect } from 'chai';

import { evaluatePhase3, REQUIRED_PHASE3_TOP_LEVEL_COMMANDS } from '../spec-node/standalonePhase3';

describe('standalone phase 3 evaluator', () => {
	it('marks phase 3 complete when native foundation checks pass', () => {
		const result = evaluatePhase3({
			rustCrate: {
				ok: true,
				cratePath: 'cmd/devcontainer-native',
				binaryName: 'devcontainer-native',
			},
			cliParity: {
				ok: true,
				topLevelCommands: [...REQUIRED_PHASE3_TOP_LEVEL_COMMANDS],
				helpParity: true,
			},
			loggingAndExitCodes: {
				ok: true,
				formats: ['text', 'json'],
				exitCodeParity: true,
			},
			compatibilityBridge: {
				ok: true,
				enabled: true,
				fallbackCommand: 'node dist/spec-node/devContainersSpecCLI.js',
				unportedCommandBehaviorVerified: true,
			},
		});

		expect(result.complete).to.equal(true);
		expect(result.summary).to.include('Phase 3 complete');
	});

	it('fails phase 3 completion when fallback bridge is missing', () => {
		const result = evaluatePhase3({
			rustCrate: {
				ok: true,
				cratePath: 'cmd/devcontainer-native',
				binaryName: 'devcontainer-native',
			},
			cliParity: {
				ok: true,
				topLevelCommands: [...REQUIRED_PHASE3_TOP_LEVEL_COMMANDS],
				helpParity: true,
			},
			loggingAndExitCodes: {
				ok: true,
				formats: ['text', 'json'],
				exitCodeParity: true,
			},
			compatibilityBridge: {
				ok: false,
				enabled: false,
				fallbackCommand: '',
				unportedCommandBehaviorVerified: false,
			},
		});

		expect(result.complete).to.equal(false);
		expect(result.missingChecks).to.deep.equal(['compatibility-bridge']);
	});
});
