import { expect } from 'chai';

import {
	formatUpstreamCompatibilityContract,
	resolvePinnedUpstreamCommit,
} from '../spec-node/migration/upstreamCompatibility';

describe('upstream compatibility contract helpers', () => {
	it('resolves pinned upstream commit using git rev-parse gitlink lookup', () => {
		const commit = resolvePinnedUpstreamCommit({
			repositoryRoot: '/workspace/devcontainer-cli',
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

	it('throws when git output is empty', () => {
		expect(() => resolvePinnedUpstreamCommit({
			repositoryRoot: '/workspace/devcontainer-cli',
			runGit: () => '   ',
		})).to.throw('Unable to resolve pinned upstream commit for upstream.');
	});
});
