/**
 * Contact Chat between two test users of the invitation demo, each in their
 * own tab of the normal host. No wallet signs in, every socket is closed and
 * every request outside this host is refused, so no chain, backend or peer is
 * reached. It needs a Chromium: playwright-core finds the one it installs, or
 * set CHROMIUM_PATH.
 *
 * What runs for real: the dev server's archive of
 * `fixtures/products/contact-chat/worker.ts`, the archive check, the
 * ordinary worker loader and frame, the product's TrUAPI storage, the core's
 * contact handles (the ones `contacts.pick` gives, which the host names
 * through the handles the core mints for it), the request service, the v2
 * draft bridge, the host's Chat screen and Inbox, and the host proofs, made
 * and checked with the keys each test user's own phrase derives after the
 * host checked them against the core's session. The chat keys a host reads
 * for its contacts come from a fixture directory, not the chain; the delivery
 * and the topics are the local simulated ones; the setup and text layer are
 * fixtures.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, Frame, Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
  openTools,
  startHost,
  testBrowser,
  useInvitationDemo,
} from "./support/fixtures.js";
import {
  TEST_USERS,
  testUserAccount,
} from "../fixtures/invitations/test-users.js";

const STEP_MS = 60_000;
const TEST_MS = 240_000;
const PRODUCT = "contact-chat.paseo";

const [ALICE, BOB] = TEST_USERS;

const ALICE_ACCOUNT = testUserAccount(ALICE!, "paseo");
const BOB_ACCOUNT = testUserAccount(BOB!, "paseo");

/** What names Alice and Bob outside a product: their names, usernames and identity accounts. */
const IDENTIFYING = [
  "alice",
  "bob",
  ALICE_ACCOUNT.slice(2),
  BOB_ACCOUNT.slice(2),
];

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

interface WorkerWindow {
  __contactChatWorker?: {
    stage: string;
    reason?: string;
    errors: string[];
    received: string[];
    starts: string[];
    record(): string | null;
  };
}

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

async function actAs(page: Page, user: string): Promise<void> {
  await useInvitationDemo(page, user);
  await ready(page);
  await page
    .locator("#session")
    .filter({ hasNotText: "Signed out" })
    .waitFor({ state: "attached" });
  await closeTools(page);
}

/** The identity account the core reports for the tab's session. */
async function sessionAccount(page: Page): Promise<string | undefined> {
  const title = (await page.locator("#session").getAttribute("title")) ?? "";
  return /Identity account (0x[0-9a-f]{64})/.exec(title)?.[1];
}

/** The contact handles Contact Chat keeps for its conversations. */
async function heldHandles(frame: Frame): Promise<string[]> {
  const record = await frame.evaluate(
    () =>
      (window as unknown as WorkerWindow).__contactChatWorker!.record() ?? "",
  );
  return [...record.matchAll(/"contact":"(0x[0-9a-f]{64})"/g)].map(
    (match) => match[1]!,
  );
}

/** Open Example Chat, press its Pick a contact, choose `name`, and read the handle `contacts.pick` gave it. */
async function pickInExampleChat(page: Page, name: string): Promise<string> {
  await openTools(page, "more");
  await page.click('[data-action="open-example-chat"]');
  const product = page.frameLocator(".test-product iframe");
  await product.locator('[data-action="pick-contact"]').waitFor();
  await closeTools(page);
  await product.locator('[data-action="pick-contact"]').click();
  const picker = page.locator("dialog.prompt", { hasText: "Choose a contact" });
  await picker.getByRole("button", { name: new RegExp(`^${name}\\b`) }).click();
  await picker.waitFor({ state: "detached" });
  const row = product.locator('[data-list="picked"] .row').first();
  await row.waitFor();
  return (await row.getAttribute("data-id"))!;
}

/** Contact Chat's worker frame, once it runs. */
async function workerFrame(page: Page): Promise<Frame> {
  const until = Date.now() + STEP_MS;
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (
        frame.url().includes("__truapi-worker.html") &&
        (await frame
          .evaluate(
            () =>
              (window as unknown as WorkerWindow).__contactChatWorker?.stage ===
              "running",
          )
          .catch(() => false))
      )
        return frame;
    await page.waitForTimeout(100);
  }
  throw new Error("Contact Chat's worker did not start");
}

/** Everything the product heard and keeps, in lower case. */
async function productVisible(frame: Frame): Promise<string> {
  const seen = await frame.evaluate(() => {
    const worker = (window as unknown as WorkerWindow).__contactChatWorker!;
    return [...worker.received, ...worker.starts, worker.record() ?? ""];
  });
  return seen.join("\n").toLowerCase();
}

async function screenshot(page: Page, name: string): Promise<void> {
  if (process.env.CHAT_SCREENSHOTS)
    await page.screenshot({
      path: `${process.env.CHAT_SCREENSHOTS}/${name}.png`,
    });
}

const chat = (page: Page) => page.click('#dock [data-destination="chat"]');
const row = (page: Page, title: string) =>
  page.locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`, {
    hasText: title,
  });

describe("Contact Chat between test users", () => {
  test(
    "invite by contact handle, accept in the host, and chat both ways; the host names everyone",
    async () => {
      const context = await browser.newContext({
        viewport: { width: 1280, height: 860 },
      });
      context.setDefaultTimeout(STEP_MS);
      await context.routeWebSocket(/.*/, (socket) => socket.close());
      const outside: string[] = [];
      await context.route(/.*/, (route) => {
        const url = route.request().url();
        if (url.startsWith(host.origin)) return route.fallback();
        outside.push(url);
        return route.abort();
      });
      const errors: string[] = [];
      const open = async () => {
        const page = await context.newPage();
        page.on("pageerror", (error) => errors.push(error.message));
        await page.goto(host.origin);
        await ready(page);
        return page;
      };
      try {
        const alice = await open();
        await actAs(alice, "alice");
        const bob = await open();
        await actAs(bob, "bob");
        // Each tab's core runs the test user's own session, whose identity
        // account the other test user's contacts name them by.
        expect([
          await sessionAccount(alice),
          await sessionAccount(bob),
        ]).toEqual([ALICE_ACCOUNT, BOB_ACCOUNT]);
        const aliceWorker = await workerFrame(alice);
        await workerFrame(bob);

        // Alice starts a chat from the host's New chat: the host's picker,
        // then the host's request form, then the host's Send prompt.
        await chat(alice);
        await alice.click('[aria-label="New chat"]');
        const chooser = alice.locator("dialog.prompt", { hasText: "New chat" });
        await chooser.locator("button", { hasText: "Bob" }).click();
        await alice.fill(".chat-new-request textarea", "Shall we talk?");
        await alice.click('[data-action="start-chat"]');
        const prompt = alice.locator("dialog.prompt", {
          hasText: "Send invitation",
        });
        expect(await prompt.innerText()).toContain("Bob");
        await prompt.getByRole("button", { name: "Send", exact: true }).click();
        await alice
          .locator(".chat-system", { hasText: "You sent a chat request" })
          .waitFor();

        // Bob's host shows who asks, in its Inbox; Accept goes to his worker.
        await chat(bob);
        await bob.click('#chat-tabs [data-chat-tab="inbox"]');
        const request = bob.locator(".notification-row").first();
        await request.waitFor();
        expect(await request.innerText()).toContain("Alice");
        expect(await request.innerText()).toContain("Shall we talk?");
        await screenshot(bob, "contact-chat-inbox");
        await request.locator('[data-action="notification-accept"]').click();
        await bob.click('#chat-tabs [data-chat-tab="chats"]');
        await row(bob, "Alice").click();
        await bob
          .locator(".chat-system", { hasText: "You accepted the chat request" })
          .waitFor();

        // Alice's host authenticated the reply, her worker checked it: she can write.
        await alice
          .locator(".chat-system", {
            hasText: "They accepted your chat request",
          })
          .waitFor();
        await alice.fill(".chat-composer-input", "Hello there");
        await alice.press(".chat-composer-input", "Enter");
        await bob
          .locator(".chat-message-in", { hasText: "Hello there" })
          .waitFor();
        await bob.fill(".chat-composer-input", "Good to see you");
        await bob.press(".chat-composer-input", "Enter");
        await alice
          .locator(".chat-message-in", { hasText: "Good to see you" })
          .waitFor();
        await screenshot(alice, "contact-chat-conversation");

        // A reload keeps both conversations, named by each host.
        await bob.reload();
        await ready(bob);
        await workerFrame(bob);
        await chat(bob);
        await row(bob, "Alice").click();
        await bob
          .locator(".chat-message-in", { hasText: "Hello there" })
          .waitFor();
        await chat(alice);
        await alice.click('[aria-label="Back to chats"]').catch(() => {});
        await row(alice, "Bob").waitFor();

        // Neither product heard or kept who the other person is.
        const bobWorker = await workerFrame(bob);
        for (const frame of [aliceWorker, bobWorker]) {
          const visible = await productVisible(frame);
          for (const value of IDENTIFYING)
            expect({ value, found: visible.includes(value) }).toEqual({
              value,
              found: false,
            });
          expect(visible).toContain("hello there");
        }
        expect(
          await aliceWorker.evaluate(
            () =>
              (window as unknown as WorkerWindow).__contactChatWorker!.errors,
          ),
        ).toEqual([]);

        // The handle each worker holds is the core's: the one `contacts.pick`
        // gives another product of the same user for the same person.
        const aliceHolds = await heldHandles(aliceWorker);
        const bobHolds = await heldHandles(bobWorker);
        expect([
          aliceHolds,
          bobHolds,
          [await pickInExampleChat(alice, "Bob")],
          [await pickInExampleChat(bob, "Alice")],
        ]).toEqual([aliceHolds, bobHolds, aliceHolds, bobHolds]);
        expect(aliceHolds).toHaveLength(1);
        expect(aliceHolds[0]).not.toBe(bobHolds[0]);
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
