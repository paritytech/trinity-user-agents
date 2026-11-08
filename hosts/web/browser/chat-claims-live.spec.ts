/**
 * LIVE, and run only with TRUAPI_LIVE=1: each tab's light client syncs Paseo
 * Next v2. It needs a Chromium: playwright-core finds the one it installs,
 * or set CHROMIUM_PATH.
 *
 * Alice's and Bob's public test accounts, each signed in as any wallet is in
 * a tab of one browser, run Contact Chat with requests between the two tabs
 * and every chat account's claim only validated, through the real UI:
 * New chat, the host's Send and Accept prompts, and each room's composer.
 * Each side's first message asks its host for its chat account's allowance;
 * the core builds the general Statement Store claim for Contact Chat's own
 * account at that chat's index, proves the person's membership, and the live
 * runtime validates it at the finalized head. Both tabs' networks refuse
 * every submission (`tools/demos/no-submit.ts`), and the test checks neither
 * was even asked for one: each message fails with the slot its claim would
 * take, and nothing leaves. A failed run keeps its evidence
 * (`browser/support/live.ts`).
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Frame, Page } from "playwright-core";
import type { Browser } from "playwright-core";
import {
  type RunningHost,
  startHost,
  testBrowser,
} from "./support/fixtures.js";
import {
  LIVE_STEP_MS,
  contactChatRunning,
  guardOf,
  keepEvidence,
  openInbox,
  sendChatRequest,
  validatingWalletTab,
  withoutBackup,
} from "./support/live.js";

const LIVE = process.env.TRUAPI_LIVE === "1";
const suite = LIVE ? describe : describe.skip;
const TEST_MS = 1_200_000;

/** What a message the host gave no allowance for says, with the slot the claim would take. */
const VALIDATED =
  /This tab only validates claims: the runtime would give this chat's account (0x[0-9a-f]{64}) slot (\d+) of (People|LitePeople), and nothing was claimed\./;

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

/** Contact Chat's worker errors in `page`. */
async function workerErrors(page: Page): Promise<string[]> {
  const frame: Frame = await contactChatRunning(page);
  return frame.evaluate(
    () =>
      (window as unknown as { __contactChatWorker: { errors: string[] } })
        .__contactChatWorker.errors,
  );
}

/** Write `text` in the open room and wait for the host's line on why it was not sent. */
async function writeAndRead(page: Page, text: string): Promise<string> {
  await page.fill(".chat-composer-input", text);
  await page.press(".chat-composer-input", "Enter");
  const line = page
    .locator(".chat-system", { hasText: "Your message was not sent" })
    .last();
  await line.waitFor();
  await page.locator(".chat-failed").last().waitFor();
  return line.innerText();
}

suite(
  "Contact Chat's chat accounts between two wallets, claims validated",
  () => {
    test(
      "each side's first message has its chat account's general claim validated live, and nothing is submitted",
      async () => {
        const context = await browser.newContext({
          viewport: { width: 1280, height: 860 },
        });
        context.setDefaultTimeout(LIVE_STEP_MS);
        await withoutBackup(context);
        const errors: string[] = [];
        const pages: Record<string, Page> = {};
        let step = "open";
        try {
          for (const name of ["alice", "bob"] as const) {
            const page = await context.newPage();
            page.on("pageerror", (error) =>
              errors.push(`${name}: ${error.message}`),
            );
            pages[name] = page;
            step = `set up ${name}`;
            await validatingWalletTab(
              page,
              host.origin,
              name,
              "local-validate",
            );
          }
          const alice = pages.alice!;
          const bob = pages.bob!;

          step = "Alice sends the request";
          await sendChatRequest(
            alice,
            "testbob.10",
            "Hello from a claims-only run",
          );

          step = "Bob accepts";
          await openInbox(bob);
          const request = bob.locator(".notification-row").first();
          await request.waitFor();
          await request.locator('[data-action="notification-accept"]').click();
          const acceptPrompt = bob.locator("dialog.prompt", {
            hasText: "Accept chat request",
          });
          await acceptPrompt.waitFor();
          await acceptPrompt.locator(".prompt-details > summary").click();
          const acceptText = await acceptPrompt.innerText();
          await acceptPrompt
            .getByRole("button", { name: "Accept", exact: true })
            .click();

          step = "Bob writes";
          await bob.click('#dock [data-destination="chat"]');
          await bob.click('#chat-tabs [data-chat-tab="chats"]');
          await bob
            .locator('[data-conversation^="chat-v2:contact-chat.paseo/"]')
            .first()
            .click();
          await bob
            .locator(".chat-system", {
              hasText: "You accepted the chat request",
            })
            .waitFor();
          const bobLine = await writeAndRead(bob, "Bob, claims only");

          step = "Alice writes";
          await alice
            .locator(".chat-system", {
              hasText: "They accepted your chat request",
            })
            .waitFor();
          const aliceLine = await writeAndRead(alice, "Alice, claims only");

          const guards = {
            alice: (await guardOf(alice))!,
            bob: (await guardOf(bob))!,
          };
          const workers = {
            alice: await workerErrors(alice),
            bob: await workerErrors(bob),
          };
          console.log(
            JSON.stringify({ acceptText, aliceLine, bobLine, guards, workers }),
          );

          const aliceClaim = VALIDATED.exec(aliceLine);
          const bobClaim = VALIDATED.exec(bobLine);
          expect([aliceClaim !== null, bobClaim !== null]).toEqual([
            true,
            true,
          ]);
          // Two chats' accounts, one per side, neither the other's.
          expect(aliceClaim![1]).not.toBe(bobClaim![1]);
          expect(acceptText).toContain(
            "Statement Store slot each day you send",
          );
          for (const guard of Object.values(guards)) {
            expect(guard.refused).toEqual([]);
            expect(
              Object.keys(guard.passed).filter((method) =>
                /submit|broadcast/i.test(method),
              ),
            ).toEqual([]);
            expect(guard.passed.state_call ?? 0).toBeGreaterThan(0);
          }
          expect(errors).toEqual([]);
        } catch (error) {
          console.log(
            `Evidence kept in ${await keepEvidence("chat-claims", pages, { step, errors, error: String(error) })}`,
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
