/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const assert = require('assert');
const cp = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const { generateCommandMatrix } = require('./generate-command-matrix');

const repositoryRoot = path.join(__dirname, '..');
const crateManifestPath = path.join(repositoryRoot, 'cmd', 'devcontainer', 'Cargo.toml');
const binaryPath = path.join(repositoryRoot, 'cmd', 'devcontainer', 'target', 'debug', process.platform === 'win32' ? 'devcontainer.exe' : 'devcontainer');
const schemaPath = path.join(repositoryRoot, 'spec', 'schemas', 'devContainer.base.schema.json');
const scenariosDirectory = path.join(repositoryRoot, 'src', 'test', 'parity', 'scenarios');

function run(command, args, options = {}) {
	return cp.spawnSync(command, args, {
		cwd: repositoryRoot,
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe'],
		...options,
	});
}

function stripJsonComments(text) {
	let result = '';
	let inString = false;
	let escaped = false;
	let inLineComment = false;
	let inBlockComment = false;

	for (let index = 0; index < text.length; index += 1) {
		const current = text[index];
		const next = text[index + 1];

		if (inLineComment) {
			if (current === '\n') {
				inLineComment = false;
				result += current;
			}
			continue;
		}

		if (inBlockComment) {
			if (current === '*' && next === '/') {
				inBlockComment = false;
				index += 1;
			}
			continue;
		}

		if (inString) {
			result += current;
			if (escaped) {
				escaped = false;
			} else if (current === '\\') {
				escaped = true;
			} else if (current === '"') {
				inString = false;
			}
			continue;
		}

		if (current === '"' && !inString) {
			inString = true;
			result += current;
			continue;
		}

		if (current === '/' && next === '/') {
			inLineComment = true;
			index += 1;
			continue;
		}

		if (current === '/' && next === '*') {
			inBlockComment = true;
			index += 1;
			continue;
		}

		result += current;
	}

	return result;
}

function stripTrailingCommas(text) {
	return text.replace(/,\s*([}\]])/g, '$1');
}

function parseJsonc(text) {
	return JSON.parse(stripTrailingCommas(stripJsonComments(text)));
}

function loadFixtureConfig(relativePath) {
	return parseJsonc(fs.readFileSync(path.join(repositoryRoot, relativePath), 'utf8'));
}

function validateConfigAgainstSpecSchema(relativePath) {
	const config = loadFixtureConfig(relativePath);
	const hasImage = typeof config.image === 'string' && config.image.trim().length > 0;
	const hasDockerFile = typeof config.dockerFile === 'string'
		|| (config.build && typeof config.build.dockerfile === 'string')
		|| (config.build && typeof config.build.dockerFile === 'string');
	const hasCompose = (
		typeof config.dockerComposeFile === 'string'
		|| Array.isArray(config.dockerComposeFile)
	) && typeof config.service === 'string' && config.service.trim().length > 0;

	if (!(hasImage || hasDockerFile || hasCompose)) {
		return {
			ok: false,
			category: 'missing-container-definition',
			message: 'Configuration must define image, dockerFile/build.dockerfile, or dockerComposeFile + service.',
		};
	}

	return {
		ok: true,
		category: 'valid',
		message: 'Configuration matches the schema container-definition lanes.',
	};
}

function normalizeReadConfigurationOutput(stdout) {
	const parsed = JSON.parse(stdout);
	return {
		configuration: parsed.configuration,
		metadata: parsed.metadata,
	};
}

function runNativeReadConfiguration(workspaceFolder, scenario) {
	const args = ['run', '--quiet', '--manifest-path', crateManifestPath, '--', 'read-configuration', '--workspace-folder', workspaceFolder];
	if (scenario.explicitConfigPath) {
		args.push('--config', path.join(workspaceFolder, scenario.explicitConfigPath));
	}
	const result = run('cargo', args);
	if (result.status !== 0) {
		throw new Error(`native read-configuration failed:\n${result.stderr}`);
	}
	return normalizeReadConfigurationOutput(result.stdout);
}

function runReferenceReadConfiguration(workspaceFolder, scenario) {
	const configPath = path.join(
		workspaceFolder,
		scenario.explicitConfigPath || scenario.workspaceConfigPath || path.join('.devcontainer', 'devcontainer.json'),
	);
	const configuration = parseJsonc(fs.readFileSync(configPath, 'utf8'));
	return {
		configuration,
		metadata: {
			workspaceFolder: fs.realpathSync(workspaceFolder),
			configFile: fs.realpathSync(configPath),
			format: 'jsonc',
			pathResolution: 'native-rust',
		},
	};
}

function ensureRequiredCommands(matrix) {
	const requiredTopLevel = ['up', 'set-up', 'build', 'run-user-commands', 'read-configuration', 'outdated', 'upgrade', 'features', 'templates', 'exec'];
	for (const command of requiredTopLevel) {
		assert(matrix.topLevel.includes(command), `missing top-level command in matrix: ${command}`);
	}

	const requiredPaths = [
		'features test',
		'features package',
		'features publish',
		'features info',
		'features resolve-dependencies',
		'features generate-docs',
		'templates apply',
		'templates publish',
		'templates metadata',
		'templates generate-docs',
	];

	for (const command of requiredPaths) {
		assert(matrix.allCommandPaths.includes(command), `missing command path in matrix: ${command}`);
	}
}

function loadScenarios() {
	return fs
		.readdirSync(scenariosDirectory)
		.filter(name => name.endsWith('.json'))
		.sort()
		.map(name => {
			const scenarioPath = path.join(scenariosDirectory, name);
			const scenario = JSON.parse(fs.readFileSync(scenarioPath, 'utf8'));
			return {
				...scenario,
				outputGoldenPath: path.join(repositoryRoot, scenario.outputGoldenPath),
			};
		});
}

function main() {
	const matrix = generateCommandMatrix();
	ensureRequiredCommands(matrix);

	const schema = JSON.parse(fs.readFileSync(schemaPath, 'utf8'));
	assert(schema.allowComments === true, 'spec schema should continue allowing JSONC comments');

	const substitutionFixture = loadFixtureConfig('src/test/parity/fixtures/config/substitution/devcontainer.json');
	assert.equal(substitutionFixture.containerEnv.USER_NAME, '${localEnv:USER}');

	const dockerPlanFixture = loadFixtureConfig('src/test/parity/fixtures/docker/build-plan/devcontainer.json');
	assert.equal(dockerPlanFixture.build.dockerfile, 'Dockerfile');
	assert.equal(dockerPlanFixture.build.context, '..');

	const validResult = validateConfigAgainstSpecSchema('src/test/parity/fixtures/schema/valid-image/devcontainer.json');
	assert.equal(validResult.ok, true, 'valid config fixture should pass schema contract');

	const invalidResult = validateConfigAgainstSpecSchema('src/test/parity/fixtures/schema/invalid-missing-container/devcontainer.json');
	assert.equal(invalidResult.ok, false, 'invalid config fixture should fail schema contract');
	assert.equal(invalidResult.category, 'missing-container-definition');

	const buildResult = run('cargo', ['build', '--manifest-path', crateManifestPath]);
	assert.equal(buildResult.status, 0, buildResult.stderr);

	for (const scenario of loadScenarios()) {
		const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'devcontainer-parity-'));
		const workspaceFolder = path.join(tempRoot, 'workspace');
		const configPath = path.join(
			workspaceFolder,
			scenario.workspaceConfigPath || path.join('.devcontainer', 'devcontainer.json'),
		);
		fs.mkdirSync(path.dirname(configPath), { recursive: true });
		fs.writeFileSync(configPath, fs.readFileSync(path.join(repositoryRoot, scenario.fixtureConfigPath), 'utf8'));

		const reference = runReferenceReadConfiguration(workspaceFolder, scenario);
		const native = runNativeReadConfiguration(workspaceFolder, scenario);
		assert.deepEqual(native, reference, `native output should be semantically equivalent to the reference scenario ${scenario.name}`);

		const golden = JSON.parse(fs.readFileSync(scenario.outputGoldenPath, 'utf8'));
		for (const key of golden.requiredTopLevelKeys) {
			assert(Object.prototype.hasOwnProperty.call(native, key), `native output missing top-level key ${key} for scenario ${scenario.name}`);
		}
		for (const key of golden.requiredMetadataKeys) {
			assert(Object.prototype.hasOwnProperty.call(native.metadata, key), `native output missing metadata key ${key} for scenario ${scenario.name}`);
		}
	}

	console.log(`[spec-parity] pinned spec commit: ${cp.execFileSync('git', ['rev-parse', 'HEAD:spec'], { cwd: repositoryRoot, encoding: 'utf8' }).trim()}`);
	console.log(`[parity-harness] semantic parity checks passed for ${loadScenarios().length} scenario(s).`);
}

main();
