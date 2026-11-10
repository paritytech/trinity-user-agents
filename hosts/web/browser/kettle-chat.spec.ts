/**
 * Kettle Chat between two test users of the invitation demo, each in their
 * own tab of the normal host, first with Kettle Chat alone in the developer
 * tools' Chat tab, then beside Contact Chat. No wallet signs in, every socket is closed and every request outside this
 * host is refused, so no chain, backend or peer is reached. It needs a
 * Chromium (playwright-core finds the one it installs, or set CHROMIUM_PATH)
 * and Kettle's WebAssembly (`npm run build:kettle`); without the WebAssembly
 * it is skipped.
 *
 * What runs for real: the dev server's archive of
 * `apps/kettle-chat/worker.ts` with Kettle's WebAssembly beside it, the
 * archive check, the ordinary worker loader and frame, Kettle's engine
 * (`paritytech/kettle-core`) one node per conversation account, the core's
 * product accounts, `createProof`, entropy and storage, the core's
 * Statement Store calls, the core's contact handles, the request service,
 * the v2 draft bridge, and the host's Chat screen and Inbox. Simulated: the
 * People node's statement methods, by a store the tabs share
 * (`fixtures/statements/local-statement-node.ts`), each conversation
 * account's allowance, and the request delivery between tabs.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
  openTools,
  startHost,
  testBrowser,
  useChatProducts,
  useInvitationDemo,
  probeChatWorkers,
} from "./support/fixtures.js";
import {
  TEST_USERS,
  testUserAccount,
} from "../fixtures/invitations/test-users.js";
import { KETTLE_WASM, KETTLE_WASM_PACKAGE } from "../apps/kettle-chat/route.js";

const STEP_MS = 60_000;
const TEST_MS = 300_000;
const PRODUCT = "kettle-chat.paseo";
const STATEMENTS = "truapi-web-signing-host:local-statements:";

const [ALICE, BOB] = TEST_USERS;
const IDENTIFYING = [
  "alice",
  "bob",
  testUserAccount(ALICE!, "paseo").slice(2),
  testUserAccount(BOB!, "paseo").slice(2),
];

const built = existsSync(
  new URL(`../${KETTLE_WASM_PACKAGE}/${KETTLE_WASM}`, import.meta.url),
);

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  if (!built) return;
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function offline(): Promise<{
  context: BrowserContext;
  outside: string[];
  errors: string[];
  open(): Promise<Page>;
}> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 860 },
  });
  context.setDefaultTimeout(STEP_MS);
  await probeChatWorkers(context);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin)) return route.fallback();
    outside.push(url);
    return route.abort();
  });
  const errors: string[] = [];
  return {
    context,
    outside,
    errors,
    open: async () => {
      const page = await context.newPage();
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(host.origin);
      await ready(page);
      return page;
    },
  };
}

interface WorkerWindow {
  __kettleChatWorker?: {
    stage: string;
    reason?: string;
    errors: string[];
    kettle?: { revision: string; protocol: number; loadMs: number };
    record(): string | null;
  };
}

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

/** Run Kettle Chat alone in this tab, then act as `user`. Each reloads the tab. */
async function actAs(page: Page, user: string): Promise<void> {
  await useChatProducts(page, { contact: false, kettle: true });
  await ready(page);
  await useInvitationDemo(page, user);
  await ready(page);
  await page
    .locator("#session")
    .filter({ hasNotText: "Signed out" })
    .waitFor({ state: "attached" });
  await closeTools(page);
}

async function workerFrame(page: Page): Promise<Frame> {
  const until = Date.now() + STEP_MS;
  let last = "";
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (frame.url().includes("__truapi-worker.html")) {
        const stage = await frame
          .evaluate(() => {
            const worker = (window as unknown as WorkerWindow)
              .__kettleChatWorker;
            return worker === undefined
              ? ""
              : `${worker.stage} ${worker.reason ?? ""}`;
          })
          .catch(() => "");
        if (stage.startsWith("running")) return frame;
        if (stage !== "") last = stage;
      }
    await page.waitForTimeout(100);
  }
  throw new Error(`Kettle Chat's worker did not start: ${last}`);
}

interface ContactWindow {
  __contactChatWorker?: {
    stage: string;
    received: string[];
    record(): string | null;
  };
}

/** Whether a Contact Chat worker frame runs in `page`. */
async function runsContactChat(page: Page): Promise<boolean> {
  for (const frame of page.frames())
    if (
      frame.url().includes("__truapi-worker.html") &&
      (await frame
        .evaluate(
          () => (window as unknown as ContactWindow).__contactChatWorker,
        )
        .then((worker) => worker !== undefined)
        .catch(() => false))
    )
      return true;
  return false;
}

/** Contact Chat's worker frame, once it runs beside Kettle Chat's. */
async function contactWorker(page: Page): Promise<Frame> {
  const until = Date.now() + STEP_MS;
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (
        frame.url().includes("__truapi-worker.html") &&
        (await frame
          .evaluate(
            () =>
              (window as unknown as ContactWindow).__contactChatWorker
                ?.stage === "running",
          )
          .catch(() => false))
      )
        return frame;
    await page.waitForTimeout(100);
  }
  throw new Error("Contact Chat's worker did not start");
}

const chat = (page: Page) => page.click('#dock [data-destination="chat"]');
const row = (page: Page, title: string) =>
  page.locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`, {
    hasText: title,
  });
const contactRows = (page: Page) =>
  page.locator('[data-conversation^="chat-v2:contact-chat.paseo/"]');
const bubble = (page: Page, text: string) =>
  page.locator(".chat-bubble-text", { hasText: text });

/** What the shared store holds: every statement's data, as text. */
async function storedText(page: Page): Promise<string> {
  return page.evaluate((prefix) => {
    const texts: string[] = [];
    for (const key of Object.keys(localStorage))
      if (key.startsWith(prefix)) texts.push(localStorage.getItem(key) ?? "");
    return texts.join("\n");
  }, STATEMENTS);
}

/** The core's question before the product's first statement: always allow it. */
async function allowStatements(page: Page): Promise<void> {
  const prompt = page.locator("dialog.prompt", { hasText: "StatementSubmit" });
  expect(await prompt.innerText()).toContain(PRODUCT);
  await prompt.getByRole("button", { name: "Always allow" }).click();
  await prompt.waitFor({ state: "detached" });
}

const utf8Hex = (text: string) => Buffer.from(text, "utf8").toString("hex");

describe("Kettle Chat between test users", () => {
  test.skipIf(!built)(
    "Kettle Chat alone: invite, accept and talk over Kettle MLS, and a reload keeps it; beside Contact Chat, New chat starts in Contact Chat, and Contact Chat off hides only its chats until it is on again",
    async () => {
      const { context, outside, errors, open } = await offline();
      try {
        const alice = await open();
        await actAs(alice, "alice");
        const bob = await open();
        await actAs(bob, "bob");
        const aliceWorker = await workerFrame(alice);
        await workerFrame(bob);
        const kettle = await aliceWorker.evaluate(
          () => (window as unknown as WorkerWindow).__kettleChatWorker!.kettle,
        );
        expect(kettle?.revision).toBe(
          "4d0764d526e5e32dfc611323e5fa522651f864e6",
        );
        expect(kettle?.protocol).toBe(6);

        await chat(alice);
        await alice.click('[aria-label="New chat"]');
        await alice
          .locator("dialog.prompt", { hasText: "New chat" })
          .locator("button", { hasText: "Bob" })
          .click();
        // Kettle Chat alone runs: New chat starts in it, and no Contact Chat
        // worker runs in either tab.
        expect([
          await runsContactChat(alice),
          await runsContactChat(bob),
        ]).toEqual([false, false]);
        await alice.fill(".chat-new-request textarea", "Shall we talk?");
        await alice.click('[data-action="start-chat"]');
        const prompt = alice.locator("dialog.prompt", {
          hasText: "Send invitation",
        });
        await prompt.getByRole("button", { name: "Send", exact: true }).click();
        await alice
          .locator(".chat-system", { hasText: "You sent a chat request" })
          .waitFor();

        await chat(bob);
        await bob.click('#chat-tabs [data-chat-tab="inbox"]');
        const request = bob.locator(".notification-row").first();
        await request.waitFor();
        expect(await request.innerText()).toContain("Alice");
        await request.locator('[data-action="notification-accept"]').click();
        // Bob's node publishes its key package before he replies: the core
        // asks him, once, to let the product post.
        await allowStatements(bob);
        await bob.click('#chat-tabs [data-chat-tab="chats"]');
        await row(bob, "Alice").click();
        // Bob writes before Alice's group exists: his host shows a composer,
        // and Kettle Chat holds the message until he joined her group.
        await bob.fill(".chat-composer-input", "Before your group");
        await bob.press(".chat-composer-input", "Enter");
        await bubble(bob, "Before your group").waitFor();

        // Alice's node makes the MLS group with the request's message; Bob's
        // joins from the Welcome, from the account the request named.
        await alice
          .locator(".chat-system", {
            hasText: "They accepted your chat request",
          })
          .waitFor();
        await allowStatements(alice);
        await bob
          .locator(".chat-system", { hasText: "The encrypted chat is set up" })
          .waitFor();
        await bubble(bob, "Shall we talk?").waitFor();
        // Once joined, Bob's held message reaches Alice, once.
        await bubble(alice, "Before your group").waitFor();

        await bob.fill(".chat-composer-input", "Yes, over MLS");
        await bob.press(".chat-composer-input", "Enter");
        await bubble(alice, "Yes, over MLS").waitFor();
        await alice.fill(".chat-composer-input", "Great");
        await alice.press(".chat-composer-input", "Enter");
        await bubble(bob, "Great").waitFor();

        // The store holds Kettle's statements, not the text.
        const stored = await storedText(alice);
        expect(stored.length).toBeGreaterThan(0);
        for (const text of ["Shall we talk?", "Yes, over MLS", "Great"]) {
          expect(stored.includes(text)).toBe(false);
          expect(stored.includes(utf8Hex(text))).toBe(false);
        }

        // Bob's tab reloads: the Chat tab keeps Kettle Chat alone, his worker
        // and node start again from storage, with the history, and the next
        // message arrives once.
        await bob.reload();
        await ready(bob);
        await workerFrame(bob);
        await openTools(bob, "chat");
        expect([
          await bob.isChecked("#chat-product-contact-chat"),
          await bob.isChecked("#chat-product-kettle-chat"),
          await runsContactChat(bob),
        ]).toEqual([false, true, false]);
        await closeTools(bob);
        await chat(bob);
        await row(bob, "Alice").click();
        await bubble(bob, "Great").waitFor();
        await alice.fill(".chat-composer-input", "After your reload");
        await alice.press(".chat-composer-input", "Enter");
        await bubble(bob, "After your reload").waitFor();
        await bob.waitForTimeout(1500);
        expect(await bubble(bob, "After your reload").count()).toBe(1);
        expect(await bubble(bob, "Great").count()).toBe(1);
        expect(await bubble(alice, "Before your group").count()).toBe(1);

        // Each product's storage is its own: the core keys it by product id.
        const keys = await alice.evaluate(() => Object.keys(localStorage));
        const kettleKeys = keys.filter((key) =>
          /:kettle-chat:(record|node):/.test(key),
        );
        expect(kettleKeys.length).toBeGreaterThan(0);
        expect(
          kettleKeys.filter((key) => !key.includes(":kettle-chat.paseo:")),
        ).toEqual([]);

        // Both products on: Kettle Chat's chat and history come back beside
        // Contact Chat, and New chat starts in Contact Chat, the default.
        for (const page of [alice, bob]) {
          await useChatProducts(page, { contact: true, kettle: true });
          await ready(page);
          await workerFrame(page);
          await contactWorker(page);
          await closeTools(page);
        }
        await chat(alice);
        await row(alice, "Bob").click();
        await bubble(alice, "After your reload").waitFor();
        await alice.click('[aria-label="Back to chats"]').catch(() => {});
        await alice.click('[aria-label="New chat"]');
        await alice
          .locator("dialog.prompt", { hasText: "New chat" })
          .locator("button", { hasText: "Bob" })
          .click();
        expect(await alice.locator('[data-focus="chat-product"]').count()).toBe(
          0,
        );
        await alice.fill(".chat-new-request textarea", "Plain chat too?");
        await alice.click('[data-action="start-chat"]');
        await alice
          .locator("dialog.prompt", { hasText: "Send invitation" })
          .getByRole("button", { name: "Send", exact: true })
          .click();

        // Turning Contact Chat off asks first while it has a request waiting.
        await openTools(alice, "chat");
        await alice
          .locator('[data-chat-product-state="contact-chat"]', {
            hasText: "1 sent request waiting",
          })
          .waitFor();
        await alice.locator("#chat-product-contact-chat").uncheck();
        const turnOff = alice.locator("dialog.prompt", {
          hasText: "Turn Contact Chat off?",
        });
        expect(await turnOff.innerText()).toContain("1 sent request");
        await turnOff.getByRole("button", { name: "Cancel" }).click();
        await turnOff.waitFor({ state: "detached" });
        expect(await alice.isChecked("#chat-product-contact-chat")).toBe(true);
        await closeTools(alice);

        // The Contact Chat request reaches Contact Chat only.
        await chat(bob);
        await bob.click('[aria-label="Back to chats"]').catch(() => {});
        await bob.click('#chat-tabs [data-chat-tab="inbox"]');
        const plain = bob.locator(".notification-row", {
          hasText: "Plain chat too?",
        });
        await plain.locator('[data-action="notification-accept"]').click();
        const bobContact = await contactWorker(bob);
        await bobContact.waitForFunction(() =>
          (
            window as unknown as ContactWindow
          ).__contactChatWorker!.received.some((line) =>
            line.includes("acceptIntent"),
          ),
        );
        const aliceContactWorker = await contactWorker(alice);
        await aliceContactWorker.waitForFunction(() =>
          (
            (
              window as unknown as ContactWindow
            ).__contactChatWorker!.record() ?? ""
          ).includes('"conversations":[{'),
        );
        const aliceContact = await aliceContactWorker.evaluate(() =>
          (window as unknown as ContactWindow).__contactChatWorker!.record(),
        );
        expect(aliceContact!.includes("0x4b06")).toBe(false);
        const kettleRecord = JSON.parse(
          (await (
            await workerFrame(bob)
          ).evaluate(() =>
            (window as unknown as WorkerWindow).__kettleChatWorker!.record(),
          ))!,
        ) as { conversations: unknown[] };
        expect(kettleRecord.conversations).toHaveLength(1);

        // Contact Chat off in Alice's tab: no Contact Chat worker and none of
        // its chats, while Kettle Chat's stays. On again, its chat is back.
        await chat(alice);
        await contactRows(alice).first().waitFor();
        await openTools(alice, "chat");
        await alice
          .locator('[data-chat-product-state="contact-chat"]', {
            hasNotText: "waiting",
          })
          .waitFor();
        await useChatProducts(alice, { contact: false, kettle: true });
        await ready(alice);
        await workerFrame(alice);
        await closeTools(alice);
        await chat(alice);
        await row(alice, "Bob").waitFor();
        expect([
          await contactRows(alice).count(),
          await runsContactChat(alice),
        ]).toEqual([0, false]);
        await useChatProducts(alice, { contact: true, kettle: true });
        await ready(alice);
        await contactWorker(alice);
        await closeTools(alice);
        await chat(alice);
        await contactRows(alice).first().waitFor();
        await row(alice, "Bob").waitFor();

        // Neither product kept who the other person is.
        const bobWorker = await workerFrame(bob);
        const aliceKettle = await workerFrame(alice);
        for (const frame of [aliceKettle, bobWorker]) {
          const record = (
            (await frame.evaluate(() =>
              (window as unknown as WorkerWindow).__kettleChatWorker!.record(),
            )) ?? ""
          ).toLowerCase();
          for (const value of IDENTIFYING)
            expect({ value, found: record.includes(value) }).toEqual({
              value,
              found: false,
            });
          expect(record).toMatch(/"peer":"0x[0-9a-f]{64}"/);
          expect(record).toMatch(/"group":"0x[0-9a-f]{64}"/);
        }
        for (const frame of [aliceKettle, bobWorker])
          expect(
            await frame.evaluate(
              () =>
                (window as unknown as WorkerWindow).__kettleChatWorker!.errors,
            ),
          ).toEqual([]);
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
