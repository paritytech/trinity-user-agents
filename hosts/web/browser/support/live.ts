import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Frame, Page } from "playwright-core";
import { closeTools, openTools } from "./fixtures.js";

/**
 * Steps the live wallet runs share: a wallet tab set up through the
 * Developer section to validate claims only, New chat through the real
 * picker, and what the tab's no-submit guard saw. It reads Paseo Next v2;
 * nothing here sets up a tab that submits.
 */

/** How long one live step may take: the light client syncs first. */
export const LIVE_STEP_MS = 240_000;

/** What the no-submit guard saw: methods let through, counted, and refused. */
export interface GuardView {
  passed: Record<string, number>;
  refused: string[];
}

export const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

/** What the tab's no-submit guard saw, or null when it has none. */
export function guardOf(page: Page): Promise<GuardView | null> {
  return page.evaluate(() => {
    const guard = (window as unknown as { __truapiNoSubmit?: GuardView })
      .__truapiNoSubmit;
    return guard === undefined
      ? null
      : { passed: guard.passed, refused: guard.refused };
  });
}

/** The host's Activity lines, newest last. */
export async function activity(page: Page): Promise<string[]> {
  await openTools(page, "inspector");
  const lines = await page
    .locator(".activity-row > .activity-head")
    .allInnerTexts();
  await closeTools(page);
  return lines;
}

/** Contact Chat's worker frame, once it runs. */
export async function contactChatRunning(page: Page): Promise<Frame> {
  const until = Date.now() + LIVE_STEP_MS;
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (
        frame.url().includes("__truapi-worker.html") &&
        (await frame
          .evaluate(
            () =>
              (
                window as unknown as {
                  __contactChatWorker?: { stage: string };
                }
              ).__contactChatWorker?.stage === "running",
          )
          .catch(() => false))
      )
        return frame;
    await page.waitForTimeout(250);
  }
  throw new Error("Contact Chat's worker did not run");
}

/**
 * Developer section: a delivery for the wallet's Contact Chat that
 * submits nothing (`validate`, the network delivery that validates claims
 * only, by default; or `local-validate`, requests between tabs of this
 * browser and chat-account claims validated); then sign in as the test
 * account `account`, and wait for Contact Chat.
 */
export async function validatingWalletTab(
  page: Page,
  origin: string,
  account: "alice" | "bob",
  delivery: "validate" | "local-validate" = "validate",
): Promise<void> {
  await page.goto(origin);
  await ready(page);
  await openTools(page, "more");
  const reload = page.waitForEvent("load");
  await page.selectOption("#wallet-delivery", delivery);
  await reload;
  await ready(page);
  await openTools(page, "account");
  await page.selectOption("#wallet", `test-account:${account}`);
  await page.click("#sign-in");
  const name = account === "alice" ? "Alice" : "Bob";
  await page
    .locator("#session")
    .filter({ hasText: `Signed in as ${name} (test account)` })
    .waitFor({ state: "attached" });
  await closeTools(page);
  await contactChatRunning(page);
}

/** Chat → New chat → find `username` → the request form → Send, and the host's Send prompt. */
export async function sendChatRequest(
  page: Page,
  username: string,
  welcome: string,
): Promise<{ firstNote: string[] }> {
  await page.click('#dock [data-destination="chat"]');
  await page.click('#chat-tabs [data-chat-tab="chats"]');
  await page.locator('#chat-screen button[aria-label="New chat"]').click();
  const picker = page.locator("dialog.prompt", { hasText: "New chat" });
  await picker.getByRole("searchbox").fill(username);
  await picker
    .getByRole("button", { name: new RegExp(username.replace(".", "\\.")) })
    .first()
    .click();
  await picker.waitFor({ state: "detached" });
  const send = page.locator('#chat-screen [data-action="start-chat"]');
  await send.waitFor();
  // What New chat said when it was drawn: a check still running shows here.
  const firstNote = await page
    .locator("#chat-screen .chat-note")
    .allInnerTexts();
  await page.locator('#chat-screen [data-focus="welcome"]').fill(welcome);
  await page
    .waitForFunction(
      () =>
        !document.querySelector<HTMLButtonElement>(
          '#chat-screen [data-action="start-chat"]',
        )?.disabled,
    )
    .catch(async (error: unknown) => {
      const note = await page
        .locator("#chat-screen .chat-note")
        .allInnerTexts();
      throw new Error(`New chat stayed blocked: ${note.join(" | ")}`, {
        cause: error,
      });
    });
  await send.click();
  await page
    .locator("dialog.prompt", { hasText: "Send invitation" })
    .getByRole("button", { name: "Send" })
    .click();
  await page
    .locator("#chat-screen")
    .filter({ hasText: "You sent a chat request" })
    .waitFor();
  return { firstNote };
}

/** Chat's Inbox, from a conversation or the list. */
export async function openInbox(page: Page): Promise<void> {
  await page.click('#dock [data-destination="chat"]');
  const back = page.locator('#chat-screen button[aria-label="Back to chats"]');
  if ((await back.count()) > 0) await back.click();
  await page.click('#chat-tabs [data-chat-tab="inbox"]');
}

/**
 * Keep what a live run saw, so a failure can be read after the fact: each
 * tab's guard record, Activity lines and a screenshot, under
 * `TRUAPI_LIVE_EVIDENCE` or the temporary directory. Returns the directory.
 */
export async function keepEvidence(
  name: string,
  pages: Record<string, Page>,
  extra: Record<string, unknown> = {},
): Promise<string> {
  const root =
    process.env.TRUAPI_LIVE_EVIDENCE ?? join(tmpdir(), "truapi-live-evidence");
  const dir = join(
    root,
    `${name}-${new Date().toISOString().replace(/[:.]/g, "-")}`,
  );
  mkdirSync(dir, { recursive: true });
  const report: Record<string, unknown> = { ...extra };
  for (const [label, page] of Object.entries(pages)) {
    const seen: Record<string, unknown> = {};
    try {
      seen.url = page.url();
      seen.guard = await guardOf(page);
      seen.activity = await activity(page);
      await page.screenshot({
        path: join(dir, `${label}.png`),
        fullPage: true,
      });
    } catch (error) {
      seen.unreadable = String(error);
    }
    report[label] = seen;
  }
  writeFileSync(join(dir, "report.json"), JSON.stringify(report, null, 2));
  return dir;
}
