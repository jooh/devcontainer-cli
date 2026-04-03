/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const { spawnSync } = require('child_process');

const [, , command, ...args] = process.argv;

if (!command) {
	console.error('Usage: node build/sanitize-env.js <command> [args...]');
	process.exit(1);
}

const env = { ...process.env };
delete env.CONFIGURATION;

const result = spawnSync(command, args, {
	stdio: 'inherit',
	env,
	shell: process.platform === 'win32',
});

if (typeof result.status === 'number') {
	process.exit(result.status);
}

process.exit(1);
