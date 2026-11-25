import { resolve } from 'node:path';

import { defineConfig } from 'vitest/config';

import { folders } from './config/index.js';

// Unit tests for E2E *helpers* (pure node code under `e2e/helpers/`, e.g. the
// sign-in retry policy) — NOT the Playwright suites, which run via
// `test:e2e:*`. Mirrors `vitest.main.config.ts`: an isolated node-env project so
// these specs run under `npm test` without pulling Playwright into the renderer
// unit run. Only import Playwright-free modules from specs discovered here.
export default defineConfig({
  cacheDir: resolve(folders.root, 'node_modules/.cache/vitest-e2e'),
  test: {
    root: folders.root,
    dir: resolve(folders.root, 'e2e'),
    globals: true,
    environment: 'node',
    include: ['**/*.spec.ts'],
    pool: 'forks',
    reporters: ['dot', 'junit'],
    outputFile: {
      junit: resolve(folders.tmp, './junit-e2e.xml'),
    },
  },
  resolve: {
    alias: {
      '@': resolve(folders.root, 'src'),
    },
  },
});
