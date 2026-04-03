import path from 'path';

export const DEFAULT_UPSTREAM_SUBMODULE_ROOT = 'upstream';

export function buildUpstreamPath(...segments: string[]) {
	return path.posix.join(DEFAULT_UPSTREAM_SUBMODULE_ROOT, ...segments);
}

export const UPSTREAM_CONTAINER_FEATURES_TEST_GLOB = buildUpstreamPath('src', 'test', 'container-features', '*.test.ts');
export const UPSTREAM_CONTAINER_FEATURES_CLI_TEST_PATH = buildUpstreamPath('src', 'test', 'container-features', 'featuresCLICommands.test.ts');
export const UPSTREAM_CONTAINER_TEMPLATES_TEST_GLOB = buildUpstreamPath('src', 'test', 'container-templates', '*.test.ts');
export const UPSTREAM_TEST_TSCONFIG_PATH = buildUpstreamPath('src', 'test', 'tsconfig.json');
