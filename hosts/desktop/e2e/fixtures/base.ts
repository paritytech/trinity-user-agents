import fs from 'fs/promises';
import os from 'os';
import path from 'path';

import { test as bddTest } from 'playwright-bdd';

import { attachFailureScreenshot, attachRecordedVideo, shutdownElectronApp } from '../helpers/artifacts';
import { clearAppData } from '../helpers/cleanup';
import { registerProductDialogHandlers } from '../helpers/dialogs';
import { type ElectronAppContext, launchElectronApp } from '../helpers/electron';
import { errorMessage } from '../helpers/errors';
import { SigningHost, slotPath } from '../helpers/signing-host';

import { setupPlatformParameter } from './allure-metadata';

export type TestFixtures = {
  /**
   * Electron app context with app instance and main window
   */
  electronApp: ElectronAppContext;

  /**
   * Temporary user data directory for this test
   */
  userDataDir: string;

  /**
   * Whether to launch with AUTOTEST mode (default: false)
   */
  autotest: boolean;
};

export type WorkerFixtures = {
  /**
   * Worker-scoped `truapi-host` signer. One slot directory per (project, worker), so
   * parallel workers never compete for the same identity's daily allowance budget.
   */
  signingHost: SigningHost;
};

/**
 * Extended test with custom fixtures
 */
export const test = bddTest.extend<TestFixtures, WorkerFixtures>({
  signingHost: [
    // eslint-disable-next-line no-empty-pattern -- Playwright fixture signature requires destructuring
    async ({}, use, workerInfo) => {
      const host = new SigningHost(slotPath(workerInfo.project.name, workerInfo.parallelIndex));
      await use(host);
      // Order matters: the process must be down before its pairings are cleared, and a
      // leaked pairing blocks this identity's rotation the next time its daily
      // allowance budget runs out.
      await host.stop();
      await host.removeDevices();
    },
    { scope: 'worker' },
  ],

  // Autotest mode flag
  autotest: [false, { option: true }],

  // User data directory - creates a temporary directory for each test
  // eslint-disable-next-line no-empty-pattern -- Playwright requires object destructuring; no deps needed
  userDataDir: async ({}, use, testInfo) => {
    const tmpDir = path.join(os.tmpdir(), 'polkadot-desktop-e2e', `test-${testInfo.workerIndex}-${Date.now()}`);
    await fs.mkdir(tmpDir, { recursive: true });

    await use(tmpDir);

    // Cleanup
    try {
      await fs.rm(tmpDir, { recursive: true, force: true });
    } catch (error) {
      console.warn(`[cleanup] temp dir ${tmpDir}: ${errorMessage(error)}`);
    }
  },

  // Electron app fixture
  electronApp: async ({ userDataDir, autotest }, use, testInfo) => {
    await setupPlatformParameter();
    const isRetry = testInfo.retry > 0;
    const flags = [autotest ? 'autotest' : '', isRetry ? `retry ${testInfo.retry} +video` : ''].filter(Boolean);
    console.info(`[test] ${testInfo.title}${flags.length ? ` (${flags.join(', ')})` : ''}`);

    const videoDir = isRetry ? path.join(testInfo.outputDir, 'video') : undefined;
    if (videoDir) await fs.mkdir(videoDir, { recursive: true });

    const context = await launchElectronApp({
      userDataDir,
      autotest,
      ...(videoDir ? { recordVideo: { dir: videoDir } } : {}),
    });

    // Clean slate — clear all storage before test actions
    await clearAppData(context.window);

    // Auto-approve transient product dialogs (permission requests, alias
    // requests) for the lifetime of this main page, so tests don't need to
    // know which products may pop them or when. Tests that need to assert on
    // the dialogs themselves opt out with the `@manual-permissions` Gherkin
    // tag in their .feature file.
    if (!testInfo.tags.includes('@manual-permissions')) {
      await registerProductDialogHandlers(context.window);
    }

    try {
      await use(context);
    } finally {
      await attachFailureScreenshot(context, testInfo);
      await shutdownElectronApp(context);
      if (videoDir) await attachRecordedVideo(context, testInfo);
    }
  },
});

export { expect } from '@playwright/test';
