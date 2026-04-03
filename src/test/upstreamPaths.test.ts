import { expect } from 'chai';

import {
	DEFAULT_UPSTREAM_SUBMODULE_ROOT,
	UPSTREAM_CONTAINER_FEATURES_CLI_TEST_PATH,
	UPSTREAM_CONTAINER_FEATURES_TEST_GLOB,
	UPSTREAM_CONTAINER_TEMPLATES_TEST_GLOB,
	UPSTREAM_TEST_TSCONFIG_PATH,
	buildUpstreamPath,
} from '../spec-node/migration/upstreamPaths';

describe('upstream path helpers', () => {
	it('builds upstream paths from canonical submodule root', () => {
		expect(buildUpstreamPath('src', 'test', 'tsconfig.json')).to.equal('upstream/src/test/tsconfig.json');
		expect(DEFAULT_UPSTREAM_SUBMODULE_ROOT).to.equal('upstream');
	});

	it('exposes shared npm script path constants for upstream test suites', () => {
		expect(UPSTREAM_TEST_TSCONFIG_PATH).to.equal('upstream/src/test/tsconfig.json');
		expect(UPSTREAM_CONTAINER_FEATURES_TEST_GLOB).to.equal('upstream/src/test/container-features/*.test.ts');
		expect(UPSTREAM_CONTAINER_FEATURES_CLI_TEST_PATH).to.equal('upstream/src/test/container-features/featuresCLICommands.test.ts');
		expect(UPSTREAM_CONTAINER_TEMPLATES_TEST_GLOB).to.equal('upstream/src/test/container-templates/*.test.ts');
	});
});
