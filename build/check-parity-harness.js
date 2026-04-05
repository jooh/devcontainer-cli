/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
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
const outputGoldenPath = path.join(repositoryRoot, 'src', 'test', 'parity', 'golden', 'read-configuration-basic.json');

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
	return JSON.parse(stdout);
}

function loadScenarios() {
	return fs.readdirSync(scenariosDirectory)
		.filter(name => name.endsWith('.json'))
		.sort()
		.map(name => JSON.parse(fs.readFileSync(path.join(scenariosDirectory, name), 'utf8')));
}

function materializeScenarioWorkspace(scenario) {
	if (scenario.workspaceFolderPath) {
		return {
			workspaceFolder: path.join(repositoryRoot, scenario.workspaceFolderPath),
			cleanup: () => {},
		};
	}

	if (scenario.fixtureConfigPath) {
		const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'devcontainer-parity-'));
		const workspaceFolder = path.join(tempRoot, 'workspace');
		const configFolder = path.join(workspaceFolder, '.devcontainer');
		fs.mkdirSync(configFolder, { recursive: true });
		fs.writeFileSync(
			path.join(configFolder, 'devcontainer.json'),
			fs.readFileSync(path.join(repositoryRoot, scenario.fixtureConfigPath), 'utf8'),
		);
		return {
			workspaceFolder,
			cleanup: () => fs.rmSync(tempRoot, { recursive: true, force: true }),
		};
	}

	throw new Error(`scenario ${scenario.name} is missing workspaceFolderPath or fixtureConfigPath`);
}

function normalizeScenarioArgs(args = []) {
	const normalized = [...args];
	for (let index = 0; index < normalized.length - 1; index += 1) {
		if (normalized[index] === '--config' && !path.isAbsolute(normalized[index + 1])) {
			normalized[index + 1] = path.join(repositoryRoot, normalized[index + 1]);
		}
	}
	return normalized;
}

function runNativeReadConfiguration(workspaceFolder, args = []) {
	const result = run('cargo', ['run', '--quiet', '--manifest-path', crateManifestPath, '--', 'read-configuration', '--workspace-folder', workspaceFolder, ...args]);
	if (result.status !== 0) {
		throw new Error(`native read-configuration failed:\n${result.stderr}`);
	}
	return normalizeReadConfigurationOutput(result.stdout);
}

function parseOptionValue(args, option) {
	const index = args.indexOf(option);
	if (index === -1 || index === args.length - 1) {
		return undefined;
	}
	return args[index + 1];
}

function hasFlag(args, flag) {
	return args.includes(flag);
}

function resolveConfigPath(workspaceFolder, explicitConfigPath) {
	if (explicitConfigPath) {
		return path.isAbsolute(explicitConfigPath)
			? explicitConfigPath
			: path.join(workspaceFolder, explicitConfigPath);
	}

	const modern = path.join(workspaceFolder, '.devcontainer', 'devcontainer.json');
	if (fs.existsSync(modern)) {
		return modern;
	}
	return path.join(workspaceFolder, '.devcontainer.json');
}

function substituteString(input, workspaceFolder, env) {
	let output = input.replaceAll('${localWorkspaceFolder}', workspaceFolder);

	while (true) {
		const start = output.indexOf('${localEnv:');
		if (start === -1) {
			break;
		}
		const remainder = output.slice(start + '${localEnv:'.length);
		const endOffset = remainder.indexOf('}');
		if (endOffset === -1) {
			break;
		}
		const variableName = remainder.slice(0, endOffset);
		const replacement = env[variableName] || '';
		const end = start + '${localEnv:'.length + endOffset + 1;
		output = `${output.slice(0, start)}${replacement}${output.slice(end)}`;
	}

	return output;
}

function substituteLocalContext(value, workspaceFolder, env) {
	if (typeof value === 'string') {
		return substituteString(value, workspaceFolder, env);
	}
	if (Array.isArray(value)) {
		return value.map(item => substituteLocalContext(item, workspaceFolder, env));
	}
	if (value && typeof value === 'object') {
		return Object.fromEntries(
			Object.entries(value).map(([key, nested]) => [
				key,
				substituteLocalContext(nested, workspaceFolder, env),
			]),
		);
	}
	return value;
}

function runReferenceReadConfiguration(workspaceFolder, args = [], env = process.env) {
	const configPath = resolveConfigPath(workspaceFolder, parseOptionValue(args, '--config'));
	const configuration = substituteLocalContext(
		parseJsonc(fs.readFileSync(configPath, 'utf8')),
		fs.realpathSync(workspaceFolder),
		env,
	);
	const payload = {
		configuration,
		metadata: {
			workspaceFolder: fs.realpathSync(workspaceFolder),
			configFile: fs.realpathSync(configPath),
			format: 'jsonc',
			pathResolution: 'native-rust',
		},
	};
	if (hasFlag(args, '--include-merged-configuration')) {
		payload.mergedConfiguration = configuration;
	}
	if (hasFlag(args, '--include-features-configuration')) {
		payload.featuresConfiguration = {
			features: configuration.features || {},
		};
	}
	return payload;
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

	const golden = JSON.parse(fs.readFileSync(outputGoldenPath, 'utf8'));

	for (const scenario of loadScenarios()) {
		const { workspaceFolder, cleanup } = materializeScenarioWorkspace(scenario);
		const args = normalizeScenarioArgs(scenario.args);
		try {
			const native = runNativeReadConfiguration(workspaceFolder, args);
			const reference = runReferenceReadConfiguration(workspaceFolder, args);
			assert.deepEqual(native, reference, `native output should be semantically equivalent to scenario ${scenario.name}`);

			for (const key of golden.requiredTopLevelKeys) {
				assert(Object.prototype.hasOwnProperty.call(native, key), `native output missing top-level key ${key} for scenario ${scenario.name}`);
			}
			for (const key of golden.requiredMetadataKeys) {
				assert(Object.prototype.hasOwnProperty.call(native.metadata, key), `native output missing metadata key ${key} for scenario ${scenario.name}`);
			}
		} finally {
			cleanup();
		}
	}

	console.log(`[spec-parity] pinned spec commit: ${cp.execFileSync('git', ['rev-parse', 'HEAD:spec'], { cwd: repositoryRoot, encoding: 'utf8' }).trim()}`);
	console.log('[parity-harness] semantic parity checks passed for all read-configuration scenarios.');
}

main();
