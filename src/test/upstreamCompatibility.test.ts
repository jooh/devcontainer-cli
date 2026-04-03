import { expect } from 'chai';

import {
	formatUpstreamCommitTraceLine,
	formatUpstreamCompatibilityContract,
	reportUpstreamCommitRegression,
	resolvePinnedUpstreamCommit,
} from '../spec-node/migration/upstreamCompatibility';

describe('upstream compatibility contract helpers', () => {
	it('resolves pinned upstream commit using git rev-parse gitlink lookup', () => {
		const commit = resolvePinnedUpstreamCommit({
			repositoryRoot: '/workspace/devcontainer-rs',
			runGit: (_cwd, args) => {
				expect(args).to.deep.equal(['rev-parse', 'HEAD:upstream']);
				return '0123456789abcdef0123456789abcdef01234567\n';
			},
		});

		expect(commit).to.equal('0123456789abcdef0123456789abcdef01234567');
	});

	it('formats compatibility contract text with the pinned upstream commit', () => {
		const contract = formatUpstreamCompatibilityContract('0123456789abcdef0123456789abcdef01234567');
		expect(contract).to.equal(
			'This repository targets upstream/ at commit 0123456789abcdef0123456789abcdef01234567.',
		);
	});

	it('formats a traceable pinned commit log line', () => {
		const line = formatUpstreamCommitTraceLine('0123456789abcdef0123456789abcdef01234567');
		expect(line).to.equal('[upstream-compat] pinned upstream commit: 0123456789abcdef0123456789abcdef01234567');
	});

	it('reports a regression summary when pinned commit changes', () => {
		const report = reportUpstreamCommitRegression({
			recordedCommit: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
			currentCommit: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
		});
		expect(report.hasRegression).to.equal(true);
		expect(report.summary).to.equal(
			'Pinned upstream commit changed from aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa to bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.',
		);
	});

	it('reports no regression when pinned commit is unchanged', () => {
		const report = reportUpstreamCommitRegression({
			recordedCommit: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
			currentCommit: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
		});
		expect(report.hasRegression).to.equal(false);
		expect(report.summary).to.equal('Pinned upstream commit unchanged at aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.');
	});

	it('throws when git output is empty', () => {
		expect(() => resolvePinnedUpstreamCommit({
			repositoryRoot: '/workspace/devcontainer-rs',
			runGit: () => '   ',
		})).to.throw('Unable to resolve pinned upstream commit for upstream.');
	});
});
