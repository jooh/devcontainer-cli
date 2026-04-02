import { expect } from 'chai';

import { evaluateNativeFoundationReadiness, REQUIRED_NATIVE_FOUNDATION_TOP_LEVEL_COMMANDS } from '../spec-node/migration/nativeFoundationReadiness';

describe('native foundation readiness evaluator', () => {
	it('marks native foundation readiness complete when native foundation checks pass', () => {
		const result = evaluateNativeFoundationReadiness({
			rustCrate: {
				ok: true,
				cratePath: 'cmd/devcontainer-native',
				binaryName: 'devcontainer-native',
			},
			cliParity: {
				ok: true,
				topLevelCommands: [...REQUIRED_NATIVE_FOUNDATION_TOP_LEVEL_COMMANDS],
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
		expect(result.summary).to.include('Native foundation readiness complete');
	});

	it('fails native foundation readiness when fallback bridge is missing', () => {
		const result = evaluateNativeFoundationReadiness({
			rustCrate: {
				ok: true,
				cratePath: 'cmd/devcontainer-native',
				binaryName: 'devcontainer-native',
			},
			cliParity: {
				ok: true,
				topLevelCommands: [...REQUIRED_NATIVE_FOUNDATION_TOP_LEVEL_COMMANDS],
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
