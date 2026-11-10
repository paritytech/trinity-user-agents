import path from 'path';
import { fileURLToPath } from 'url';

import { DEFAULT_ENVIRONMENT_ID, networkTld } from '../helpers/dotns';
import { type StaticServer, startStaticServer } from '../helpers/http-server';

import { test as baseTest } from './base';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export type LinkTestsTarget = {
  /** Tab identifier after opening — matches the URL parser's `identifier` field. */
  identifier: string;
  /** What to fill into the address bar. Includes the `/link-tests` path. */
  address: string;
  /** Async teardown (closes embedded server if one was started). */
  close: () => Promise<void>;
};

type LinkTestsWorkerFixtures = {
  linkTestsTarget: LinkTestsTarget;
};

/**
 * Resolves a Link Tests target. Two modes:
 *
 *  - Default: starts a worker-scoped HTTP server serving the plain HTML
 *    fixture at `e2e/test-products/link-tests/` on an ephemeral port.
 *  - `LINK_TESTS_URL=<url>` env var set: uses that URL (typically coin-flip's
 *    dev server, e.g. `http://localhost:5000`). No server is started.
 *
 * Both fixtures expose the same `window.__linkTests` API and data-testids,
 * so the feature file / steps work against either.
 */
export const test = baseTest.extend<object, LinkTestsWorkerFixtures>({
  linkTestsTarget: [
    // eslint-disable-next-line no-empty-pattern -- Playwright requires object destructuring
    async ({}, use) => {
      const externalUrl = process.env['LINK_TESTS_URL'];

      if (externalUrl) {
        const parsed = new URL(externalUrl);
        const identifier = parsed.port ? `${parsed.hostname}:${parsed.port}` : parsed.hostname;
        await use({
          identifier,
          address: `${identifier}/link-tests`,
          close: async () => {},
        });
        return;
      }

      const fixturePath = path.resolve(__dirname, '../test-products/link-tests');
      // The fixture's cross-product href has to end in the suffix the host will
      // recognise, or the link reads as an ordinary navigation and replaces the
      // tab instead of opening a new one.
      const server: StaticServer = await startStaticServer(fixturePath, {
        '{{TLD}}': networkTld(DEFAULT_ENVIRONMENT_ID),
      });
      const identifier = `localhost:${server.port}`;
      await use({
        identifier,
        address: `${identifier}/link-tests`,
        close: () => server.close(),
      });
      await server.close();
    },
    { scope: 'worker' },
  ],
});

export { expect } from '@playwright/test';
