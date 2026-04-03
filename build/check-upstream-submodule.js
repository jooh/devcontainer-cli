/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');
const cp = require('child_process');

const repositoryRoot = path.join(__dirname, '..');
const upstreamRoot = path.join(repositoryRoot, 'upstream');
const requiredUpstreamFiles = [
	'package.json',
	'src/spec-node/devContainersSpecCLI.ts',
];

function fail(message) {
	console.error(message);
	console.error('Run: git submodule update --init --recursive');
	process.exit(1);
}

if (!fs.existsSync(upstreamRoot) || !fs.statSync(upstreamRoot).isDirectory()) {
	fail('Missing upstream/ submodule directory.');
}

for (const relativePath of requiredUpstreamFiles) {
	const absolutePath = path.join(upstreamRoot, relativePath);
	if (!fs.existsSync(absolutePath)) {
		fail(`Missing upstream submodule asset: upstream/${relativePath}`);
	}
}

try {
	const status = cp.execFileSync('git', ['submodule', 'status', '--', 'upstream'], {
		cwd: repositoryRoot,
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe'],
	}).trim();
	if (!status) {
		fail('Unable to determine upstream submodule status.');
	}
	if (status.startsWith('-')) {
		fail('upstream submodule is not initialized.');
	}
} catch (error) {
	fail(`Unable to resolve upstream submodule status: ${error instanceof Error ? error.message : 'unknown error'}`);
}

console.log('Upstream submodule check passed.');
