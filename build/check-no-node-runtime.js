/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');

const repositoryRoot = path.join(__dirname, '..');
const runtimeFiles = [
	'cmd/devcontainer/src/main.rs',
	'scripts/standalone/build.sh',
];

const forbiddenPatterns = [
	/devContainersSpecCLI\.js/,
	/Command::new\("node"\)/,
	/exec node /,
];

for (const relativePath of runtimeFiles) {
	const content = fs.readFileSync(path.join(repositoryRoot, relativePath), 'utf8');
	for (const pattern of forbiddenPatterns) {
		if (pattern.test(content)) {
			console.error(`[no-node-runtime] forbidden runtime reference ${pattern} found in ${relativePath}`);
			process.exit(1);
		}
	}
}

console.log('[no-node-runtime] native runtime contains no Node bridge references.');
