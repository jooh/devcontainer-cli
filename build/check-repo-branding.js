/*---------------------------------------------------------------------------------------------
 *  Copyright (c) devcontainer-rs contributors.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const cp = require('child_process');
const fs = require('fs');
const path = require('path');

const repositoryRoot = path.join(__dirname, '..');
const ignoredFiles = new Set([
	'build/check-repo-branding.js',
]);
const ignoredPrefixes = [
	'spec',
	'spec/',
	'upstream',
	'upstream/',
];

const forbiddenPatterns = [
	/\bMicrosoft\b/i,
	/\bmcr\.microsoft\.com\b/i,
	/\b(?:go\.)?microsoft\.com\b/i,
	/\baka\.ms\b/i,
];

function run(command, args) {
	const result = cp.spawnSync(command, args, {
		cwd: repositoryRoot,
		encoding: 'utf8',
		stdio: ['ignore', 'pipe', 'pipe'],
	});

	if (result.error || result.status !== 0) {
		const output = [result.stdout, result.stderr].filter(Boolean).join('\n').trim();
		throw new Error(output || `${command} ${args.join(' ')} failed`);
	}

	return result.stdout;
}

function shouldScan(relativePath) {
	if (ignoredFiles.has(relativePath)) {
		return false;
	}

	return !ignoredPrefixes.some(prefix => relativePath.startsWith(prefix));
}

function main() {
	const trackedFiles = run('git', ['ls-files', '-z'])
		.split('\0')
		.filter(Boolean)
		.filter(shouldScan);

	const failures = [];

	for (const relativePath of trackedFiles) {
		const absolutePath = path.join(repositoryRoot, relativePath);
		if (!fs.statSync(absolutePath).isFile()) {
			continue;
		}
		const content = fs.readFileSync(absolutePath, 'utf8');
		for (const pattern of forbiddenPatterns) {
			if (pattern.test(content)) {
				failures.push(`${relativePath}: ${pattern}`);
			}
		}
	}

	if (failures.length > 0) {
		console.error('[repo-branding] repository-owned files still contain forbidden branding or Microsoft-hosted URLs:');
		for (const failure of failures) {
			console.error(`  - ${failure}`);
		}
		process.exit(1);
	}

	console.log('[repo-branding] repository-owned files contain no Microsoft branding.');
}

main();
