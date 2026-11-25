import { type TestInfo } from '@playwright/test';

import { type ElectronAppContext, closeElectronApp, getRecordedVideoPath } from './electron';
import { errorMessage } from './errors';

/**
 * Best-effort Electron teardown. Never throws — it logs a warning and continues, so a
 * failed close doesn't abort the rest of a fixture's cleanup.
 *
 * The signer is torn down separately, by whichever fixture owns its `SigningHost`: its
 * pairings must be cleared after the process is stopped, which is a different lifetime
 * from the Electron window's.
 */
export async function shutdownElectronApp(context: ElectronAppContext): Promise<void> {
  await closeElectronApp(context).catch(err => console.warn(`[cleanup] close app: ${errorMessage(err)}`));
}

/**
 * Attach a PNG screenshot of the Electron window to the Allure result when the
 * test has failed. Best-effort: if the window has already been disposed
 * (crashed Electron, earlier `waitFor*` timeout), logs and swallows the error.
 */
export async function attachFailureScreenshot(context: ElectronAppContext, testInfo: TestInfo): Promise<void> {
  if (testInfo.status === testInfo.expectedStatus) return;

  try {
    const screenshot = await context.window.screenshot();
    await testInfo.attach('screenshot', { body: screenshot, contentType: 'image/png' });
  } catch (err) {
    console.warn(`[artifact] screenshot skipped, window likely closed: ${errorMessage(err)}`);
  }
}

// A two-client chat scenario runs for minutes and the app is chatty; keep the
// tail rather than the head — the throw we're chasing is always at the end.
const CONSOLE_TAIL_LINES = 2_000;

/**
 * Start buffering the renderer's console output and uncaught errors, and return
 * a reader for what has accumulated.
 *
 * Playwright reports a failed step's locator state but nothing the app itself
 * logged, so an app-level rejection (`console.error` in a catch, an unhandled
 * page error) leaves no trace in the run — the UI shows its friendly copy and
 * the cause is gone. Pair with `attachFailureConsole` at teardown.
 */
export function recordRendererConsole(context: ElectronAppContext): () => string {
  const lines: string[] = [];

  const push = (line: string) => {
    lines.push(line);
    if (lines.length > CONSOLE_TAIL_LINES) lines.shift();
  };

  context.window.on('console', message => push(`[${message.type()}] ${message.text()}`));
  context.window.on('pageerror', error => push(`[pageerror] ${error.stack ?? error.message}`));

  return () => lines.join('\n');
}

/**
 * Attach the buffered renderer console to the Allure result when the test has
 * failed. No-op when nothing was logged.
 */
export async function attachFailureConsole(readConsole: () => string, testInfo: TestInfo, label: string): Promise<void> {
  if (testInfo.status === testInfo.expectedStatus) return;

  const body = readConsole();
  if (!body) return;

  await testInfo
    .attach(`${label}-console`, { body, contentType: 'text/plain' })
    .catch(err => console.warn(`[artifact] console attach: ${errorMessage(err)}`));
}

/**
 * Attach the recorded `.webm` video to the Allure result.
 *
 * Must be called AFTER `shutdownElectronApp` — Playwright writes the file on
 * app close, so `video().path()` only resolves then.
 */
export async function attachRecordedVideo(context: ElectronAppContext, testInfo: TestInfo): Promise<void> {
  const videoPath = await getRecordedVideoPath(context);
  if (!videoPath) {
    console.warn('[artifact] no recorded video path');
    return;
  }
  await testInfo
    .attach('video', { path: videoPath, contentType: 'video/webm' })
    .catch(err => console.warn(`[artifact] video attach: ${errorMessage(err)}`));
}
