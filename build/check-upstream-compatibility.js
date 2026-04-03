/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');
const cp = require('child_process');

const repositoryRoot = path.join(__dirname, '..');
const baselineFilePath = path.join(repositoryRoot, 'docs/upstream/compatibility-baseline.json');
const submodulePath = 'upstream';

function fail(message) {
	console.error(message);
	process.exit(1);
}

function resolveCurrentPinnedCommit() {
	return cp.execFileSync('git', ['rev-parse', `:${submodulePath}`], {
		cwd: repositoryRoot,
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe'],
	}).trim();
}

if (!fs.existsSync(baselineFilePath)) {
	fail(`Missing compatibility baseline file: ${path.relative(repositoryRoot, baselineFilePath)}`);
}

const baselineRaw = fs.readFileSync(baselineFilePath, 'utf8');
/** @type {{ submodulePath?: string; pinnedCommit?: string; compatibilityContract?: string; }} */
const baseline = JSON.parse(baselineRaw);

if (!baseline.pinnedCommit || typeof baseline.pinnedCommit !== 'string') {
	fail(`Invalid pinnedCommit in ${path.relative(repositoryRoot, baselineFilePath)}`);
}

const currentCommit = resolveCurrentPinnedCommit();
const recordedCommit = baseline.pinnedCommit.trim();

console.log(`[upstream-compat] pinned upstream commit: ${currentCommit}`);

if (recordedCommit !== currentCommit) {
	fail(
		[
			`Pinned upstream commit changed from ${recordedCommit} to ${currentCommit}.`,
			'Run parity tests and update docs/upstream/compatibility-baseline.json in the same change.',
		].join('\n'),
	);
}

console.log('[upstream-compat] compatibility baseline matches pinned upstream commit.');
