/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');

const { generateCliMetadata } = require('./generate-cli-metadata');

const repositoryRoot = path.join(__dirname, '..');
const outputPath = path.join(repositoryRoot, 'TODO_ARGS.md');

function optionDisplay(option) {
	const aliases = option.aliases.length
		? ` (aliases: ${option.aliases.map(alias => `\`-${alias}\``).join(', ')})`
		: '';
	const visibility = option.visible ? '' : ' [hidden upstream option]';
	const description = option.description || 'No upstream help description available.';
	return `- \`--${option.name}\`${aliases}${visibility}: ${description}`;
}

function renderTodoArgs(metadata) {
	const commands = metadata.commands
		.filter(
			command =>
				command.unsupportedOptions.length || command.unsupportedPositionals.length,
		)
		.sort((left, right) => left.path.localeCompare(right.path));

	return [
		'# TODO_ARGS',
		'',
		'Unsupported CLI args for the current pinned upstream command surface.',
		'',
		`- Upstream commit: \`${metadata.upstreamCommit}\``,
		`- Source: \`${metadata.sourcePath}\``,
		'',
		...commands.flatMap(command => {
			const optionsByName = new Map(command.options.map(option => [option.name, option]));
			const optionEntries = command.unsupportedOptions.map(name => {
				const option = optionsByName.get(name) || {
					name,
					aliases: [],
					description: null,
					visible: false,
				};
				return optionDisplay(option);
			});
			const positionalEntries = command.unsupportedPositionals.map(
				name => `- positional \`${name}\``,
			);
			return [
				`## \`${command.path}\``,
				'',
				...optionEntries,
				...positionalEntries,
				'',
			];
		}),
	].join('\n');
}

function writeTodoArgs(text) {
	fs.writeFileSync(outputPath, `${text}\n`);
}

function compareToCommitted(text) {
	if (!fs.existsSync(outputPath)) {
		throw new Error(`Missing committed TODO args file: ${path.relative(repositoryRoot, outputPath)}`);
	}
	return fs.readFileSync(outputPath, 'utf8') === `${text}\n`;
}

if (require.main === module) {
	const metadata = generateCliMetadata();
	const text = renderTodoArgs(metadata);
	if (process.argv.includes('--check')) {
		if (!compareToCommitted(text)) {
			console.error('Committed TODO_ARGS.md is out of date. Run node build/generate-todo-args.js');
			process.exit(1);
		}
		console.log('[todo-args] committed TODO_ARGS.md matches current metadata.');
	} else {
		writeTodoArgs(text);
		console.log(`[todo-args] wrote ${path.relative(repositoryRoot, outputPath)}`);
	}
}

module.exports = {
	renderTodoArgs,
	writeTodoArgs,
};
