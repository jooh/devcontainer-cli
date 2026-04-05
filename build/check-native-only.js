/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const cp = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const repositoryRoot = path.join(__dirname, '..');
const crateRoot = path.join(repositoryRoot, 'cmd', 'devcontainer');
const binaryPath = path.join(crateRoot, 'target', 'debug', process.platform === 'win32' ? 'devcontainer.exe' : 'devcontainer');

function run(command, args, options = {}) {
	return cp.spawnSync(command, args, {
		cwd: repositoryRoot,
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe'],
		...options,
	});
}

function fail(message, result) {
	if (result) {
		if (result.stdout) {
			console.error(result.stdout.trimEnd());
		}
		if (result.stderr) {
			console.error(result.stderr.trimEnd());
		}
	}
	console.error(message);
	process.exit(1);
}

function assertSuccess(label, result) {
	if (result.error || result.status !== 0) {
		fail(`[native-only] ${label} failed.`, result);
	}
}

function assertFailure(label, result, pattern) {
	if (result.error || result.status === 0) {
		fail(`[native-only] ${label} unexpectedly succeeded.`, result);
	}
	if (!pattern.test(`${result.stdout}\n${result.stderr}`)) {
		fail(`[native-only] ${label} did not emit expected output.`, result);
	}
}

function sanitizedPath() {
	const defaultSegments = process.platform === 'win32'
		? [process.env.SystemRoot ? path.join(process.env.SystemRoot, 'System32') : 'C:\\Windows\\System32']
		: ['/usr/bin', '/bin', '/usr/sbin', '/sbin'];
	return defaultSegments.join(path.delimiter);
}

const buildResult = run('cargo', ['build', '--manifest-path', path.join('cmd', 'devcontainer', 'Cargo.toml')]);
assertSuccess('cargo build', buildResult);

const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'devcontainer-native-only-'));
const workspaceFolder = path.join(tempRoot, 'workspace');
const configFolder = path.join(workspaceFolder, '.devcontainer');
fs.mkdirSync(configFolder, { recursive: true });
fs.writeFileSync(path.join(configFolder, 'devcontainer.json'), '{\n  // comment preserved for JSONC path resolution\n  "name": "native-only-check"\n}\n');

const env = {
	...process.env,
	PATH: sanitizedPath(),
};

assertSuccess('top-level help without node', run(binaryPath, ['--help'], { env }));
assertSuccess('build help without node', run(binaryPath, ['build', '--help'], { env }));
assertSuccess('read-configuration help without node', run(binaryPath, ['read-configuration', '--help'], { env }));
assertSuccess('native read-configuration without node', run(binaryPath, ['read-configuration', '--workspace-folder', workspaceFolder], { env }));
assertSuccess('native features list without node', run(binaryPath, ['features', 'list'], { env }));
assertSuccess('native templates list without node', run(binaryPath, ['templates', 'list'], { env }));

assertFailure(
	'native-only fallback block',
	run(binaryPath, ['features', 'apply'], {
		env: {
			...env,
			DEVCONTAINER_NATIVE_ONLY: '1',
		},
	}),
	/(Native-only mode forbids Node fallback|Unsupported features subcommand)/,
);

console.log('[native-only] startup contract satisfied without node on PATH.');
