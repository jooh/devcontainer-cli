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

const pickConfigProperties = [
	'onCreateCommand',
	'updateContentCommand',
	'postCreateCommand',
	'postStartCommand',
	'postAttachCommand',
	'waitFor',
	'customizations',
	'mounts',
	'containerEnv',
	'containerUser',
	'init',
	'privileged',
	'capAdd',
	'securityOpt',
	'remoteUser',
	'userEnvProbe',
	'remoteEnv',
	'overrideCommand',
	'portsAttributes',
	'otherPortsAttributes',
];

const pickFeatureProperties = [
	'onCreateCommand',
	'updateContentCommand',
	'postCreateCommand',
	'postStartCommand',
	'postAttachCommand',
	'init',
	'privileged',
	'capAdd',
	'securityOpt',
	'customizations',
];

const replaceProperties = [
	'customizations',
	'entrypoint',
	'onCreateCommand',
	'updateContentCommand',
	'postCreateCommand',
	'postStartCommand',
	'postAttachCommand',
	'shutdownAction',
];

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

function pickProperties(value, keys) {
	return Object.fromEntries(
		keys
			.filter(key => Object.prototype.hasOwnProperty.call(value, key))
			.map(key => [key, value[key]]),
	);
}

function mergeLegacyFeatureCustomizations(feature) {
	if (!feature.extensions && !feature.settings) {
		return feature;
	}
	const copy = { ...feature };
	const customizations = copy.customizations || (copy.customizations = {});
	const vscode = customizations.vscode || (customizations.vscode = {});
	if (copy.extensions) {
		vscode.extensions = (vscode.extensions || []).concat(copy.extensions);
		delete copy.extensions;
	}
	if (copy.settings) {
		vscode.settings = {
			...copy.settings,
			...(vscode.settings || {}),
		};
		delete copy.settings;
	}
	return copy;
}

function buildFeaturesConfiguration(workspaceFolder, configPath, configuration) {
	const features = configuration.features;
	if (!features || typeof features !== 'object' || Array.isArray(features)) {
		return undefined;
	}

	const allowedParent = path.join(workspaceFolder, '.devcontainer');
	const configDir = path.dirname(configPath);
	const featureSets = Object.entries(features).map(([userFeatureId, userValue]) => {
		if (path.isAbsolute(userFeatureId) || !userFeatureId.startsWith('.')) {
			throw new Error(`reference read-configuration only supports local relative features in parity scenarios (unsupported: ${userFeatureId})`);
		}
		const featureFolder = path.join(configDir, userFeatureId);
		const relative = path.relative(allowedParent, featureFolder);
		if (relative.startsWith('..') || path.isAbsolute(relative)) {
			throw new Error(`local feature path must remain under ${allowedParent}: ${featureFolder}`);
		}
		const featureDefinition = parseJsonc(fs.readFileSync(path.join(featureFolder, 'devcontainer-feature.json'), 'utf8'));
		const feature = mergeLegacyFeatureCustomizations({
			...featureDefinition,
			id: path.basename(userFeatureId),
			name: userFeatureId,
			value: userValue,
			included: true,
		});
		return {
			sourceInformation: {
				type: 'file-path',
				resolvedFilePath: featureFolder,
				userFeatureId,
			},
			features: [feature],
			internalVersion: '2',
		};
	});

	return {
		featureSets,
	};
}

function mergeObjectProperty(entries, key) {
	return Object.assign({}, ...entries.map(entry => entry[key] || {}));
}

function collectPropertyValues(entries, key) {
	const values = entries
		.map(entry => entry[key])
		.filter(value => value !== undefined);
	return values.length ? values : undefined;
}

function mergeUniqueArrayProperty(entries, key) {
	const values = [];
	for (const entry of entries) {
		for (const value of entry[key] || []) {
			if (!values.some(existing => JSON.stringify(existing) === JSON.stringify(value))) {
				values.push(value);
			}
		}
	}
	return values.length ? values : undefined;
}

function mergeCustomizations(entries) {
	const customizations = {};
	for (const entry of entries) {
		if (!entry.customizations) {
			continue;
		}
		for (const [key, value] of Object.entries(entry.customizations)) {
			(customizations[key] ||= []).push(value);
		}
	}
	return Object.keys(customizations).length ? customizations : undefined;
}

function findLastProperty(entries, key) {
	return [...entries].reverse().find(entry => entry[key] !== undefined)?.[key];
}

function featureMetadataEntries(featuresConfiguration) {
	return (featuresConfiguration?.featureSets || []).flatMap(featureSet =>
		featureSet.features.map(feature => ({
			id: featureSet.sourceInformation.userFeatureId,
			...pickProperties(feature, pickFeatureProperties),
		})),
	);
}

function mergeConfiguration(configuration, featuresConfiguration) {
	const imageMetadata = [
		...featureMetadataEntries(featuresConfiguration),
		pickProperties(configuration, pickConfigProperties),
	].filter(entry => Object.keys(entry).length);
	const customizations = mergeCustomizations(imageMetadata);
	const merged = { ...configuration };
	for (const property of replaceProperties) {
		delete merged[property];
	}

	merged.init = imageMetadata.some(entry => entry.init);
	merged.privileged = imageMetadata.some(entry => entry.privileged);
	merged.remoteEnv = mergeObjectProperty(imageMetadata, 'remoteEnv');
	merged.containerEnv = mergeObjectProperty(imageMetadata, 'containerEnv');
	merged.portsAttributes = mergeObjectProperty(imageMetadata, 'portsAttributes');

	if (customizations) {
		merged.customizations = customizations;
	}
	if (mergeUniqueArrayProperty(imageMetadata, 'capAdd')) {
		merged.capAdd = mergeUniqueArrayProperty(imageMetadata, 'capAdd');
	}
	if (mergeUniqueArrayProperty(imageMetadata, 'securityOpt')) {
		merged.securityOpt = mergeUniqueArrayProperty(imageMetadata, 'securityOpt');
	}
	if (collectPropertyValues(imageMetadata, 'entrypoint')) {
		merged.entrypoints = collectPropertyValues(imageMetadata, 'entrypoint');
	}
	if (collectPropertyValues(imageMetadata, 'onCreateCommand')) {
		merged.onCreateCommands = collectPropertyValues(imageMetadata, 'onCreateCommand');
	}
	if (collectPropertyValues(imageMetadata, 'updateContentCommand')) {
		merged.updateContentCommands = collectPropertyValues(imageMetadata, 'updateContentCommand');
	}
	if (collectPropertyValues(imageMetadata, 'postCreateCommand')) {
		merged.postCreateCommands = collectPropertyValues(imageMetadata, 'postCreateCommand');
	}
	if (collectPropertyValues(imageMetadata, 'postStartCommand')) {
		merged.postStartCommands = collectPropertyValues(imageMetadata, 'postStartCommand');
	}
	if (collectPropertyValues(imageMetadata, 'postAttachCommand')) {
		merged.postAttachCommands = collectPropertyValues(imageMetadata, 'postAttachCommand');
	}
	if (findLastProperty(imageMetadata, 'waitFor') !== undefined) {
		merged.waitFor = findLastProperty(imageMetadata, 'waitFor');
	}
	if (findLastProperty(imageMetadata, 'remoteUser') !== undefined) {
		merged.remoteUser = findLastProperty(imageMetadata, 'remoteUser');
	}
	if (findLastProperty(imageMetadata, 'containerUser') !== undefined) {
		merged.containerUser = findLastProperty(imageMetadata, 'containerUser');
	}
	if (findLastProperty(imageMetadata, 'userEnvProbe') !== undefined) {
		merged.userEnvProbe = findLastProperty(imageMetadata, 'userEnvProbe');
	}
	if (findLastProperty(imageMetadata, 'overrideCommand') !== undefined) {
		merged.overrideCommand = findLastProperty(imageMetadata, 'overrideCommand');
	}
	if (findLastProperty(imageMetadata, 'otherPortsAttributes') !== undefined) {
		merged.otherPortsAttributes = findLastProperty(imageMetadata, 'otherPortsAttributes');
	}
	if (findLastProperty(imageMetadata, 'shutdownAction') !== undefined) {
		merged.shutdownAction = findLastProperty(imageMetadata, 'shutdownAction');
	}

	return merged;
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
	const featuresConfiguration = hasFlag(args, '--include-features-configuration') || hasFlag(args, '--include-merged-configuration')
		? buildFeaturesConfiguration(fs.realpathSync(workspaceFolder), fs.realpathSync(configPath), configuration)
		: undefined;
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
		payload.mergedConfiguration = mergeConfiguration(configuration, featuresConfiguration);
	}
	if (hasFlag(args, '--include-features-configuration') && featuresConfiguration) {
		payload.featuresConfiguration = featuresConfiguration;
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
