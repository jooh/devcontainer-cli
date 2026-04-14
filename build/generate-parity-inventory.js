/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');

const repositoryRoot = path.join(__dirname, '..');
const commandMatrixPath = path.join(repositoryRoot, 'docs', 'upstream', 'command-matrix.json');
const cliSourcePath = path.join(repositoryRoot, 'cmd', 'devcontainer', 'src', 'cli.rs');
const commandsModPath = path.join(repositoryRoot, 'cmd', 'devcontainer', 'src', 'commands', 'mod.rs');
const collectionsModPath = path.join(repositoryRoot, 'cmd', 'devcontainer', 'src', 'commands', 'collections', 'mod.rs');
const outputJsonPath = path.join(repositoryRoot, 'docs', 'upstream', 'parity-inventory.json');
const outputMarkdownPath = path.join(repositoryRoot, 'docs', 'upstream', 'parity-inventory.md');

const KNOWN_GAPS = {
	'up': [
		'Native runtime now layers Features for image, dockerfile, and Docker Compose configs.',
		'Several upstream flags remain unimplemented or are only partially honored.',
	],
	'set-up': [
		'Lifecycle execution is native, but several upstream setup and dotfiles flags are still missing.',
	],
	'build': [
		'Native runtime now layers Features for image, dockerfile, and Docker Compose configs.',
		'Several upstream build flags are still unimplemented or are only partially honored.',
	],
	'run-user-commands': [
		'Lifecycle execution is native, but several upstream runtime and dotfiles flags are still missing.',
	],
	'read-configuration': [
		'`--include-features-configuration` resolves local/published Feature sets natively, but still relies on fixture/manual manifests rather than full OCI resolution.',
		'Variable substitution support is still narrower than upstream.',
	],
	'outdated': [
		'Backed by fixture/manual catalog data rather than real upstream registry resolution.',
	],
	'upgrade': [
		'Backed by fixture/manual catalog data rather than real upstream registry resolution.',
	],
	'features': [
		'Top-level command exists, but several subcommands still use local/offline substitutes rather than real OCI flows.',
	],
	'features test': [
		'Native test runner exists, but parity with upstream feature resolution and registry-backed dependencies is incomplete.',
	],
	'features package': [
		'Packages local targets, but broader upstream collection behavior is still limited.',
	],
	'features publish': [
		'Publishes a local OCI layout rather than a real authenticated registry push flow.',
	],
	'features info': [
		'Info modes are native, but published metadata still comes from embedded/manual catalog data instead of real OCI fetches.',
	],
	'features resolve-dependencies': [
		'Current implementation follows declared `dependsOn` edges, but still relies on local/manual manifests rather than full OCI graph resolution.',
	],
	'features generate-docs': [
		'Documentation generation is minimal compared with upstream.',
	],
	'templates': [
		'Top-level command exists, but published-template flows still rely on embedded/local substitutes.',
	],
	'templates apply': [
		'Published template application is still based on embedded/local substitutes instead of real OCI fetches.',
	],
	'templates publish': [
		'Publishes a local OCI layout rather than a real authenticated registry push flow.',
	],
	'templates metadata': [
		'Published template metadata is still based on embedded/local substitutes instead of real OCI fetches.',
	],
	'templates generate-docs': [
		'Documentation generation is minimal compared with upstream.',
	],
	'exec': [
		'Core exec path is native, but upstream option coverage is still narrower.',
	],
};

const COMMAND_SOURCE_PATHS = {
	'up': [
		'cmd/devcontainer/src/runtime',
		'cmd/devcontainer/src/commands/configuration',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'set-up': [
		'cmd/devcontainer/src/runtime',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'build': [
		'cmd/devcontainer/src/runtime',
		'cmd/devcontainer/src/commands/configuration',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'run-user-commands': [
		'cmd/devcontainer/src/runtime',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'read-configuration': [
		'cmd/devcontainer/src/commands/configuration',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/runtime',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'outdated': [
		'cmd/devcontainer/src/commands/configuration',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'upgrade': [
		'cmd/devcontainer/src/commands/configuration',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'features': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'features test': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'features package': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'features publish': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'features info': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'features resolve-dependencies': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'features generate-docs': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'templates': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'templates apply': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
	'templates publish': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'templates metadata': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'templates generate-docs': [
		'cmd/devcontainer/src/commands/collections',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/cli.rs',
	],
	'exec': [
		'cmd/devcontainer/src/runtime',
		'cmd/devcontainer/src/commands/exec.rs',
		'cmd/devcontainer/src/commands/common.rs',
		'cmd/devcontainer/src/commands/mod.rs',
		'cmd/devcontainer/src/cli.rs',
		'cmd/devcontainer/src/config.rs',
	],
};

const OPTION_EVIDENCE_OVERRIDES = {
	'build': {
		'omit-syntax-directive': [
			'cmd/devcontainer/src/commands/common/args.rs',
			'cmd/devcontainer/src/runtime/build.rs',
		],
		'skip-persisting-customizations-from-features': [
			'cmd/devcontainer/src/commands/common/args.rs',
			'cmd/devcontainer/src/runtime/mod.rs',
		],
	},
	'up': {
		'omit-config-remote-env-from-metadata': [
			'cmd/devcontainer/src/commands/common/args.rs',
			'cmd/devcontainer/src/runtime/compose/override_file.rs',
			'cmd/devcontainer/src/runtime/container/engine_run.rs',
			'cmd/devcontainer/src/runtime/metadata.rs',
		],
	},
};

const OPTION_EVIDENCE_EXCLUSIONS = {
	'up': {
		'dotfiles-target-path': ['cmd/devcontainer/src/cli.rs'],
	},
	'set-up': {
		'dotfiles-target-path': ['cmd/devcontainer/src/cli.rs'],
	},
	'run-user-commands': {
		'dotfiles-target-path': ['cmd/devcontainer/src/cli.rs'],
	},
};

function readFile(relativePath) {
	return fs.readFileSync(path.join(repositoryRoot, relativePath), 'utf8');
}

function walkFiles(relativePath) {
	const absolutePath = path.join(repositoryRoot, relativePath);
	const stat = fs.statSync(absolutePath);
	if (stat.isFile()) {
		return [relativePath];
	}
	return fs.readdirSync(absolutePath)
		.sort()
		.flatMap(entry => walkFiles(path.join(relativePath, entry)));
}

function commandSourceFiles(commandPath) {
	const configuredPaths = COMMAND_SOURCE_PATHS[commandPath];
	if (!configuredPaths) {
		throw new Error(`No source-path mapping configured for command path: ${commandPath}`);
	}
	return configuredPaths.flatMap(relativePath => walkFiles(relativePath));
}

function optionEvidence(commandPath, optionName) {
	const needle = `--${optionName}`;
	const overrideEvidence = OPTION_EVIDENCE_OVERRIDES[commandPath]?.[optionName] || [];
	const excludedEvidence = OPTION_EVIDENCE_EXCLUSIONS[commandPath]?.[optionName] || [];
	return [...new Set([...commandSourceFiles(commandPath), ...overrideEvidence])]
		.filter(relativePath => readFile(relativePath).includes(needle))
		.concat(overrideEvidence)
		.filter(relativePath => !excludedEvidence.includes(relativePath))
		.filter((relativePath, index, allPaths) => allPaths.indexOf(relativePath) === index)
		.sort();
}

function declaredTopLevelCommand(command) {
	const cliSource = fs.readFileSync(cliSourcePath, 'utf8');
	const commandsMod = fs.readFileSync(commandsModPath, 'utf8');
	return cliSource.includes(`"${command}"`) && commandsMod.includes(`"${command}"`);
}

function declaredCollectionSubcommand(group, subcommand) {
	const collectionsSource = fs.readFileSync(collectionsModPath, 'utf8');
	return collectionsSource.includes(`"${subcommand}"`);
}

function commandDeclared(pathValue) {
	const segments = pathValue.split(' ');
	if (segments.length === 1) {
		return declaredTopLevelCommand(pathValue);
	}
	const [group, subcommand] = segments;
	if (group === 'features' || group === 'templates') {
		return declaredCollectionSubcommand(group, subcommand);
	}
	return false;
}

function buildInventory() {
	const matrix = JSON.parse(fs.readFileSync(commandMatrixPath, 'utf8'));
	const inventory = matrix.commands.map(command => {
		const declared = commandDeclared(command.path);
		const options = command.options.map(optionName => {
			const evidence = optionEvidence(command.path, optionName);
			return {
				name: optionName,
				sourceReferenced: evidence.length > 0,
				evidence,
			};
		});
		const optionSummary = {
			total: options.length,
			referenced: options.filter(option => option.sourceReferenced).length,
			missing: options.filter(option => !option.sourceReferenced).length,
		};
		return {
			group: command.group,
			path: command.path,
			description: command.description,
			declared,
			optionSummary,
			options,
			knownGaps: KNOWN_GAPS[command.path] || [],
		};
	});

	const totalOptions = inventory.reduce((sum, command) => sum + command.optionSummary.total, 0);
	const referencedOptions = inventory.reduce((sum, command) => sum + command.optionSummary.referenced, 0);
	return {
		upstreamCommit: matrix.upstreamCommit,
		sourcePath: matrix.sourcePath,
		summary: {
			commandPathsTotal: inventory.length,
			commandPathsDeclared: inventory.filter(command => command.declared).length,
			optionsTotal: totalOptions,
			optionsReferenced: referencedOptions,
			optionsMissing: totalOptions - referencedOptions,
		},
		commands: inventory,
	};
}

function renderMarkdown(report) {
	const lines = [
		'# Native Parity Inventory',
		'',
		'Generated from the pinned upstream CLI command matrix and static source evidence in the Rust implementation.',
		'',
		`- Upstream commit: \`${report.upstreamCommit}\``,
		`- Source: \`${report.sourcePath}\``,
		`- Declared upstream command paths present natively: \`${report.summary.commandPathsDeclared}/${report.summary.commandPathsTotal}\``,
		`- Upstream options with a native source reference in mapped files: \`${report.summary.optionsReferenced}/${report.summary.optionsTotal}\``,
		'',
		'This report is a static inventory, not a semantic parity proof. A referenced option can still be only partially implemented, and command-level known gaps are called out explicitly below.',
		'',
		'## Summary',
		'',
		'| Command | Declared | Option refs | Missing refs | Known gaps |',
		'| --- | --- | --- | --- | --- |',
	];

	for (const command of report.commands) {
		lines.push(
			`| \`${command.path}\` | ${command.declared ? 'yes' : 'no'} | ${command.optionSummary.referenced}/${command.optionSummary.total} | ${command.optionSummary.missing} | ${command.knownGaps.length} |`
		);
	}

	for (const command of report.commands) {
		lines.push('');
		lines.push(`## \`${command.path}\``);
		lines.push('');
		lines.push(`- Description: ${command.description}`);
		lines.push(`- Declared natively: ${command.declared ? 'yes' : 'no'}`);
		lines.push(
			`- Option source references: ${command.optionSummary.referenced}/${command.optionSummary.total}`
		);

		const missingOptions = command.options
			.filter(option => !option.sourceReferenced)
			.map(option => `\`${option.name}\``);
		lines.push(
			`- Missing option references: ${missingOptions.length ? missingOptions.join(', ') : 'none'}`
		);
		if (command.knownGaps.length) {
			lines.push(`- Known gaps: ${command.knownGaps.join(' ')}`);
		}
	}

	return `${lines.join('\n')}\n`;
}

function compareToCommitted(report) {
	const generatedJson = `${JSON.stringify(report, null, '\t')}\n`;
	const generatedMarkdown = renderMarkdown(report);
	if (!fs.existsSync(outputJsonPath) || !fs.existsSync(outputMarkdownPath)) {
		return false;
	}
	return fs.readFileSync(outputJsonPath, 'utf8') === generatedJson
		&& fs.readFileSync(outputMarkdownPath, 'utf8') === generatedMarkdown;
}

function writeReport(report) {
	fs.writeFileSync(outputJsonPath, `${JSON.stringify(report, null, '\t')}\n`);
	fs.writeFileSync(outputMarkdownPath, renderMarkdown(report));
}

if (require.main === module) {
	const report = buildInventory();
	if (process.argv.includes('--check')) {
		if (!compareToCommitted(report)) {
			console.error('Committed parity inventory is out of date. Run node build/generate-parity-inventory.js');
			process.exit(1);
		}
		console.log('[parity-inventory] committed parity inventory matches the current source.');
	} else {
		writeReport(report);
		console.log(`[parity-inventory] wrote ${path.relative(repositoryRoot, outputJsonPath)} and ${path.relative(repositoryRoot, outputMarkdownPath)}`);
	}
}

module.exports = {
	buildInventory,
	renderMarkdown,
	writeReport,
};
