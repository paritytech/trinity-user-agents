/**
 * LIVE, and run only with TRUAPI_LIVE=1: the host's light client syncs Paseo
 * Next v2, and the picker reads usernames through the dev server. It needs a
 * Chromium: playwright-core finds the one it installs, or set CHROMIUM_PATH.
 *
 * Alice's public test account, signed in as any wallet is, runs Contact Chat
 * over the network delivery with claims only validated, through the real UI:
 * Developer section, Chat's New chat, the picker's username search, the
 * request form and the host's Send prompt. The tab's network refuses every
 * submission (`tools/demos/no-submit.ts`), and the test checks it was
 * never even asked for one: the request ends paused with the claim the
 * runtime would accept, Nova chat never starts, and nothing leaves. A failed
 * run keeps its evidence (`browser/support/live.ts`).
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, Page } from "playwright-core";
import {
  type RunningHost,
  startHost,
  testBrowser,
} from "./support/fixtures.js";
import {
  LIVE_STEP_MS,
  guardOf,
  keepEvidence,
  openInbox,
  sendChatRequest,
  validatingWalletTab,
} from "./support/live.js";

const LIVE = process.env.TRUAPI_LIVE === "1";
const suite = LIVE ? describe : describe.skip;
const TEST_MS = 900_000;

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  if (!LIVE) return;
  host = await startHost();
  browser = await testBrowser();
}, LIVE_STEP_MS);

afterAll(async () => {
  await host?.close();
});

suite(
  "a wallet's Contact Chat over the network delivery, validating only",
  () => {
    test(
      "New chat starts Contact Chat, its request is validated and paused, and nothing is submitted",
      async () => {
        const context = await browser.newContext({
          viewport: { width: 1280, height: 860 },
        });
        context.setDefaultTimeout(LIVE_STEP_MS);
        const errors: string[] = [];
        let page: Page | null = null;
        let step = "open";
        try {
          page = await context.newPage();
          page.on("pageerror", (error) => errors.push(error.message));
          step = "set up Alice";
          await validatingWalletTab(page, host.origin, "alice");
          step = "send the request";
          const sent = await sendChatRequest(
            page,
            "testbob.10",
            "Hello from a validate-only run",
          );
          step = "read the Inbox";
          await openInbox(page);
          const problem = page.locator(
            '[data-group="sent"] .notification-problem',
          );
          await problem
            .filter({ hasText: "claims are only validated" })
            .waitFor();
          const paused = await problem.first().innerText();
          const guard = (await guardOf(page))!;
          const engine = await page
            .locator("#chat-screen")
            .getAttribute("data-chat-engine");
          console.log(
            JSON.stringify({ paused, guard, firstNote: sent.firstNote }),
          );

          expect(paused).toMatch(
            /^not posted: claims are only validated\. Today's claim would take slot \d+ of (People|LitePeople)\.$/,
          );
          expect(guard.refused).toEqual([]);
          expect(
            Object.keys(guard.passed).filter((method) =>
              /submit|broadcast/i.test(method),
            ),
          ).toEqual([]);
          expect(guard.passed.state_call ?? 0).toBeGreaterThan(0);
          expect(engine).toBe("off");
          expect(errors).toEqual([]);
        } catch (error) {
          if (page !== null)
            console.log(
              `Evidence kept in ${await keepEvidence("validate-only", { alice: page }, { step, errors, error: String(error) })}`,
            );
          throw error;
        } finally {
          await context.close();
        }
      },
      TEST_MS,
    );
  },
);
