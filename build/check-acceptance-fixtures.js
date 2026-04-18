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
const repoReadmePath = path.join(repositoryRoot, 'README.md');
const architecturePath = path.join(repositoryRoot, 'docs', 'architecture.md');
const makefilePath = path.join(repositoryRoot, 'Makefile');

const expectedScenarioIds = [
	'image-lifecycle',
	'dockerfile-build',
	'template-node-mongo',
	'local-feature',
	'published-feature',
];

const allowedChecks = new Set([
	'templates-apply',
	'read-configuration',
	'build',
	'up',
	'exec',
	'run-user-commands',
	'set-up',
]);

function assertExists(targetPath, message) {
	assert(fs.existsSync(targetPath), message);
}

function readJson(filePath) {
	return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function validateScenarioCommon(scenario) {
	assert.equal(typeof scenario.id, 'string', 'scenario.id must be a string');
	assert.equal(typeof scenario.kind, 'string', `scenario ${scenario.id} must declare kind`);
	assert.equal(typeof scenario.description, 'string', `scenario ${scenario.id} must declare description`);
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
	assert(
		Array.isArray(scenario.expectedFiles) && scenario.expectedFiles.length > 0,
		`scenario ${scenario.id} must declare expectedFiles`,
	);

	for (const check of scenario.checks) {
		assert(
			allowedChecks.has(check),
			`scenario ${scenario.id} uses an unsupported check token: ${check}`,
		);
	}

	assertExists(
		path.join(repositoryRoot, scenario.path),
		`scenario path must exist: ${scenario.path}`,
	);
	assertExists(
		path.join(repositoryRoot, scenario.workspacePath),
		`scenario workspace path must exist: ${scenario.workspacePath}`,
	);

	for (const expectedFile of scenario.expectedFiles) {
		assertExists(
			path.join(repositoryRoot, scenario.path, expectedFile),
			`scenario ${scenario.id} is missing expected file: ${expectedFile}`,
		);
	}
}

function validateWorkspaceScenario(scenario, expectation) {
	const configPath = path.join(repositoryRoot, scenario.path, '.devcontainer', 'devcontainer.json');
	assertExists(configPath, `workspace scenario ${scenario.id} must include .devcontainer/devcontainer.json`);
	const config = readJson(configPath);

	if (expectation.image) {
		assert.equal(typeof config.image, 'string', `${scenario.id} must be image-based`);
		assert(!config.build, `${scenario.id} must not declare a build block`);
	}

	if (expectation.build) {
		assert(config.build, `${scenario.id} must declare a build block`);
		assert.equal(
			config.build.dockerfile,
			'Dockerfile',
			`${scenario.id} must build from .devcontainer/Dockerfile`,
		);
	}

	if (expectation.lifecycle) {
		for (const key of [
			'onCreateCommand',
			'updateContentCommand',
			'postCreateCommand',
			'postStartCommand',
			'postAttachCommand',
		]) {
			assert.equal(
				typeof config[key],
				'string',
				`${scenario.id} must declare ${key}`,
			);
		}
	}

	if (expectation.localFeature) {
		assertExists(
			path.join(repositoryRoot, scenario.path, '.devcontainer', 'local-feature', 'devcontainer-feature.json'),
			`${scenario.id} must include a local feature manifest`,
		);
		assertExists(
			path.join(repositoryRoot, scenario.path, '.devcontainer', 'local-feature', 'install.sh'),
			`${scenario.id} must include a local feature install script`,
		);
		assert(
			config.features && Object.prototype.hasOwnProperty.call(config.features, './local-feature'),
			`${scenario.id} must reference ./local-feature`,
		);
	}

	if (expectation.publishedFeature) {
		const featureIds = Object.keys(config.features || {});
		assert(featureIds.length > 0, `${scenario.id} must declare at least one published feature`);
		assert(
			featureIds.some((featureId) => featureId.startsWith('ghcr.io/devcontainers/features/')),
			`${scenario.id} must use a ghcr.io/devcontainers/features/* identifier`,
		);
	}
}

function validateTemplateScenario(scenario) {
	assert.equal(scenario.kind, 'template', `${scenario.id} must be a template scenario`);
	assert(
		Array.isArray(scenario.postApplyFiles) && scenario.postApplyFiles.length > 0,
		`${scenario.id} must declare postApplyFiles`,
	);
	assert(scenario.template, `${scenario.id} must declare template metadata`);
	assert.equal(
		scenario.template.id,
		'ghcr.io/devcontainers/templates/node-mongo:latest',
		`${scenario.id} must use the embedded node-mongo template`,
	);
	assert.deepStrictEqual(
		scenario.template.args,
		{},
		`${scenario.id} must keep template args empty for the baseline fixture`,
	);
	assert.deepStrictEqual(
		scenario.template.features,
		[],
		`${scenario.id} must keep template extra features empty for the baseline fixture`,
	);
	assert(
		scenario.checks.includes('templates-apply'),
		`${scenario.id} must include templates-apply in its checks`,
	);
}

function validateTemplateReadmeCommands(scenario, acceptanceReadme) {
	const workspaceBasename = path.basename(scenario.workspacePath);
	const workspaceFolder = `/workspaces/${workspaceBasename}`;
	const expectedExecCommand = `devcontainer exec --workspace-folder ${scenario.workspacePath} /bin/sh -lc 'ls ${workspaceFolder}/.devcontainer'`;
	assert(
		acceptanceReadme.includes(expectedExecCommand),
		`acceptance/README.md must use ${workspaceFolder} for the ${scenario.id} exec check`,
	);
}

function main() {
	assertExists(acceptanceRoot, 'acceptance/ must exist');
	assertExists(readmePath, 'acceptance/README.md must exist');
	assertExists(manifestPath, 'acceptance/scenarios.json must exist');
	assertExists(repoReadmePath, 'README.md must exist');
	assertExists(architecturePath, 'docs/architecture.md must exist');
	assertExists(makefilePath, 'Makefile must exist');

	const manifest = readJson(manifestPath);
	const acceptanceReadme = fs.readFileSync(readmePath, 'utf8');
	const repoReadme = fs.readFileSync(repoReadmePath, 'utf8');
	const architecture = fs.readFileSync(architecturePath, 'utf8');
	const makefile = fs.readFileSync(makefilePath, 'utf8');
	assert(Array.isArray(manifest), 'acceptance/scenarios.json must contain a JSON array');
	assert.deepStrictEqual(
		manifest.map((scenario) => scenario.id),
		expectedScenarioIds,
		'acceptance/scenarios.json must list the expected scenarios in a stable order',
	);

	const templateScenarios = manifest.filter((scenario) => scenario.kind === 'template');
	assert.equal(templateScenarios.length, 1, 'acceptance suite must include exactly one template scenario');

	const networkScenarios = manifest.filter((scenario) => scenario.requiresNetwork);
	assert.equal(networkScenarios.length, 1, 'acceptance suite must include exactly one network scenario');
	assert.equal(networkScenarios[0].id, 'published-feature', 'published-feature must be the only network scenario');

	for (const scenarioId of expectedScenarioIds) {
		assert(
			acceptanceReadme.includes(scenarioId),
			`acceptance/README.md must describe ${scenarioId}`,
		);
	}
	assert(
		acceptanceReadme.includes('devcontainer templates apply'),
		'acceptance/README.md must document the template apply command',
	);
	assert(
		acceptanceReadme.includes('ghcr.io/devcontainers/templates/node-mongo:latest'),
		'acceptance/README.md must mention the embedded node-mongo template id',
	);
	assert(
		acceptanceReadme.includes('acceptance/template-node-mongo/workspace'),
		'acceptance/README.md must mention the generated template workspace path',
	);
	assert(repoReadme.includes('acceptance/'), 'README.md must mention acceptance/');
	assert(
		architecture.includes('acceptance/'),
		'docs/architecture.md must mention acceptance/',
	);
	assert(
		/^tests:.*acceptance-fixtures-check/m.test(makefile),
		'Makefile tests target must include acceptance-fixtures-check',
	);

	for (const scenario of manifest) {
		validateScenarioCommon(scenario);
	}

	validateWorkspaceScenario(manifest[0], { image: true, lifecycle: true });
	validateWorkspaceScenario(manifest[1], { build: true });
	validateTemplateScenario(manifest[2]);
	validateTemplateReadmeCommands(manifest[2], acceptanceReadme);
	validateWorkspaceScenario(manifest[3], { image: true, localFeature: true });
	validateWorkspaceScenario(manifest[4], { image: true, publishedFeature: true });

	console.log(
		`[acceptance-fixtures] suite layout looks current (${manifest.length} scenario(s)).`,
	);
}

main();
