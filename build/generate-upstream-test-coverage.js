/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');

const { renderMarkdown } = require('./check-upstream-test-coverage');

const repositoryRoot = path.join(__dirname, '..');
const coverageMapPath = path.join(repositoryRoot, 'docs', 'upstream', 'test-coverage-map.json');
const coverageMarkdownPath = path.join(repositoryRoot, 'docs', 'upstream', 'test-coverage-map.md');

function main() {
	const report = JSON.parse(fs.readFileSync(coverageMapPath, 'utf8'));
	fs.writeFileSync(coverageMarkdownPath, renderMarkdown(report));
	console.log(`[upstream-test-coverage] wrote ${path.relative(repositoryRoot, coverageMarkdownPath)}`);
}

main();
