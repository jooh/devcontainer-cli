/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

'use strict';

const fs = require('fs');
const path = require('path');

const specNodeRoot = path.join(__dirname, '..', 'src', 'spec-node');
const migrationNamespace = path.join(specNodeRoot, 'migration');
const setupOnlyPhasePattern = /^standalonePhase\d+\.ts$/;

const offenders = fs.readdirSync(specNodeRoot)
	.filter(entry => setupOnlyPhasePattern.test(entry))
	.map(entry => path.join(specNodeRoot, entry));

if (offenders.length) {
	console.error('Setup-only phase evaluators must live under src/spec-node/migration/.');
	offenders.forEach(offender => {
		const relative = path.relative(path.join(__dirname, '..'), offender);
		console.error(` - ${relative}`);
	});
	process.exit(1);
}

if (!fs.existsSync(migrationNamespace)) {
	console.error('Expected migration namespace at src/spec-node/migration/.');
	process.exit(1);
}

console.log('Setup namespace separation check passed.');
