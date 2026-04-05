/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const cp = require('child_process');
const fs = require('fs');
const path = require('path');

const repositoryRoot = path.join(__dirname, '..');
const baselinePath = path.join(repositoryRoot, 'docs', 'upstream', 'schema-parity-baseline.json');

function fail(message) {
	console.error(message);
	process.exit(1);
}

if (!fs.existsSync(baselinePath)) {
	fail(`Missing schema parity baseline: ${path.relative(repositoryRoot, baselinePath)}`);
}

const baseline = JSON.parse(fs.readFileSync(baselinePath, 'utf8'));
const currentSpecCommit = cp.execFileSync('git', ['rev-parse', 'HEAD:spec'], {
	cwd: repositoryRoot,
	encoding: 'utf8',
	stdio: ['ignore', 'pipe', 'pipe'],
}).trim();

if (baseline.specCommit !== currentSpecCommit) {
	fail(
		[
			`Pinned spec commit changed from ${baseline.specCommit} to ${currentSpecCommit}.`,
			'Update schema parity fixtures/tests and docs/upstream/schema-parity-baseline.json in the same change.',
		].join('\n'),
	);
}

console.log(`[spec-parity] pinned spec commit: ${currentSpecCommit}`);
console.log('[spec-parity] schema parity baseline matches pinned spec commit.');
