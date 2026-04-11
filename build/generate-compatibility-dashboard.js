/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');

const repositoryRoot = path.join(__dirname, '..');
const compatibilityBaselinePath = path.join(repositoryRoot, 'docs', 'upstream', 'compatibility-baseline.json');
const schemaParityBaselinePath = path.join(repositoryRoot, 'docs', 'upstream', 'schema-parity-baseline.json');
const parityInventoryPath = path.join(repositoryRoot, 'docs', 'upstream', 'parity-inventory.json');
const outputPath = path.join(repositoryRoot, 'docs', 'upstream', 'compatibility-dashboard.md');

const HIGHLIGHTED_GAP_COMMANDS = [
	'build',
	'read-configuration',
	'outdated',
	'features',
];

function readJson(filePath) {
	return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function buildDashboard() {
	const compatibilityBaseline = readJson(compatibilityBaselinePath);
	const schemaParityBaseline = readJson(schemaParityBaselinePath);
	const parityInventory = readJson(parityInventoryPath);

	const highlightedGaps = HIGHLIGHTED_GAP_COMMANDS
		.map(commandPath => parityInventory.commands.find(command => command.path === commandPath))
		.filter(command => command && command.knownGaps.length > 0)
		.map(command => ({
			path: command.path,
			summary: command.knownGaps.join(' '),
		}));

	return {
		upstreamCommit: compatibilityBaseline.pinnedCommit,
		specCommit: schemaParityBaseline.specCommit,
		commandMatrixSource: 'docs/upstream/command-matrix.json',
		parityInventoryPath: 'docs/upstream/parity-inventory.md',
		summary: parityInventory.summary,
		highlightedGaps,
		guardrails: [
			'cargo test --manifest-path cmd/devcontainer/Cargo.toml',
			'npm test',
			'node build/generate-cli-reference.js --check',
			'node build/generate-parity-inventory.js --check',
			'node build/generate-compatibility-dashboard.js --check',
			'node build/check-native-only.js',
			'node build/check-parity-harness.js',
			'node build/check-spec-drift.js',
			'node build/check-no-node-runtime.js',
		],
	};
}

function renderMarkdown(report) {
	const lines = [
		'# Native Compatibility Dashboard',
		'',
		`- Pinned upstream commit: \`${report.upstreamCommit}\``,
		`- Pinned spec commit: \`${report.specCommit}\``,
		`- Command matrix source: \`${report.commandMatrixSource}\``,
		`- Native parity inventory: \`${report.parityInventoryPath}\``,
		'',
		'## Current snapshot',
		'',
		`- Declared upstream command paths present natively: \`${report.summary.commandPathsDeclared}/${report.summary.commandPathsTotal}\``,
		`- Upstream options with a native source reference in mapped Rust sources: \`${report.summary.optionsReferenced}/${report.summary.optionsTotal}\``,
		'- The parity inventory is a static source-evidence report. It is intended to identify obvious gaps and track drift, not to claim semantic parity by itself.',
		'',
		'## Highest-Impact Gaps',
		'',
	];

	for (const gap of report.highlightedGaps) {
		lines.push(`- \`${gap.path}\`: ${gap.summary}`);
	}

	lines.push('');
	lines.push('## Guardrails');
	lines.push('');

	for (const guardrail of report.guardrails) {
		lines.push(`- \`${guardrail}\``);
	}

	return `${lines.join('\n')}\n`;
}

function compareToCommitted(report) {
	if (!fs.existsSync(outputPath)) {
		return false;
	}
	return fs.readFileSync(outputPath, 'utf8') === renderMarkdown(report);
}

function writeReport(report) {
	fs.writeFileSync(outputPath, renderMarkdown(report));
}

if (require.main === module) {
	const report = buildDashboard();
	if (process.argv.includes('--check')) {
		if (!compareToCommitted(report)) {
			console.error('Committed compatibility dashboard is out of date. Run node build/generate-compatibility-dashboard.js');
			process.exit(1);
		}
		console.log('[compatibility-dashboard] committed dashboard matches the current source.');
	} else {
		writeReport(report);
		console.log(`[compatibility-dashboard] wrote ${path.relative(repositoryRoot, outputPath)}`);
	}
}

module.exports = {
	buildDashboard,
	renderMarkdown,
	writeReport,
};
