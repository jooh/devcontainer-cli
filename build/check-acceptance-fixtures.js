/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const assert = require('assert');
const fs = require('fs');
const path = require('path');

const repositoryRoot = path.join(__dirname, '..');
const acceptanceRoot = path.join(repositoryRoot, 'acceptance');
const manifestPath = path.join(acceptanceRoot, 'scenarios.json');
const readmePath = path.join(acceptanceRoot, 'README.md');

function assertExists(targetPath, message) {
	assert(fs.existsSync(targetPath), message);
}

function main() {
	assertExists(acceptanceRoot, 'acceptance/ must exist');
	assertExists(readmePath, 'acceptance/README.md must exist');
	assertExists(manifestPath, 'acceptance/scenarios.json must exist');

	const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
	assert(Array.isArray(manifest), 'acceptance/scenarios.json must contain a JSON array');
	assert(manifest.length > 0, 'acceptance/scenarios.json must list at least one scenario');

	for (const scenario of manifest) {
		assert.equal(typeof scenario.id, 'string', 'scenario.id must be a string');
		assert.equal(typeof scenario.kind, 'string', `scenario ${scenario.id} must declare kind`);
		assert.equal(typeof scenario.path, 'string', `scenario ${scenario.id} must declare path`);
		assert.equal(
			typeof scenario.workspacePath,
			'string',
			`scenario ${scenario.id} must declare workspacePath`,
		);
		assert.equal(
			typeof scenario.requiresNetwork,
			'boolean',
			`scenario ${scenario.id} must declare requiresNetwork`,
		);
		assert(
			Array.isArray(scenario.checks) && scenario.checks.length > 0,
			`scenario ${scenario.id} must declare at least one check`,
		);

		assertExists(
			path.join(repositoryRoot, scenario.path),
			`scenario path must exist: ${scenario.path}`,
		);
		assertExists(
			path.join(repositoryRoot, scenario.workspacePath),
			`scenario workspace path must exist: ${scenario.workspacePath}`,
		);
	}

	console.log(
		`[acceptance-fixtures] basic suite metadata looks current (${manifest.length} scenario(s)).`,
	);
}

main();
