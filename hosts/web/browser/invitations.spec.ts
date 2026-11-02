/**
 * Headless-browser test of invitations between test users, each in its own
 * tab of the normal host, sent from Chat's New chat through the demo's chat
 * worker, Contact Chat. No wallet signs in, every socket is closed and every
 * request outside this host is refused, so no chain, backend or peer is
 * reached. It needs a Chromium: playwright-core finds the one it installs, or
 * set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type { Browser, BrowserContext, Page } from "playwright-core";
import {
  type RunningHost,
  startHost,
  testBrowser,
  closeTools,
  openTools,
  type DevTab,
  useInvitationDemo,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 240_000;
const PRODUCT = "contact-chat.paseo";

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

interface Session {
  context: BrowserContext;
  outside: string[];
  errors: string[];
  open(): Promise<Page>;
}

async function session(
  viewport = { width: 1280, height: 860 },
): Promise<Session> {
  const context = await browser.newContext({ viewport });
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

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

async function setMenu(
  page: Page,
  open: boolean,
  tab: DevTab = "account",
): Promise<void> {
  if (open) await openTools(page, tab);
  else await closeTools(page);
}

/**
 * More → Invitation demo → the user, or Off with "". The tab reloads as that
 * user; nothing opens.
 */
async function actAs(page: Page, user: string): Promise<void> {
  await useInvitationDemo(page, user === "" ? null : user);
  await ready(page);
  if (user !== "")
    await page
      .locator("#session")
      .filter({ hasNotText: "Signed out" })
      .waitFor({ state: "attached" });
  await setMenu(page, false);
}

async function menuAction(page: Page, action: string): Promise<void> {
  await setMenu(page, true, "more");
  await page.click(`[data-action="${action}"]`);
  await setMenu(page, false);
}

const dockTab = (destination: string) =>
  `#dock [data-destination="${destination}"]`;

const inboxTab = '#chat-tabs [data-chat-tab="inbox"]';

/** The Inbox's count, kept on Chat's Inbox tab whichever screen shows. */
async function badge(page: Page): Promise<number> {
  return page
    .locator(`${inboxTab} [data-badge]`)
    .evaluate((node) =>
      (node as HTMLElement).hidden ? 0 : Number(node.textContent),
    );
}

/** Chat, at its Inbox tab. */
async function openInbox(page: Page): Promise<void> {
  await page.click(dockTab("chat"));
  const back = page.locator('[aria-label="Back to chats"]');
  if (await back.isVisible()) await back.click();
  await page.click(inboxTab);
}

/** Chat, at its conversation list. */
async function openChats(page: Page): Promise<void> {
  await openInbox(page);
  await page.click('#chat-tabs [data-chat-tab="chats"]');
}

/** Contact Chat's room with `title`, in Chat's list. */
const room = (page: Page, title: string) =>
  page.locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`, {
    hasText: title,
  });

/** A system line in the open room. */
const systemLine = (page: Page, text: string) =>
  page.locator(".chat-system", { hasText: text });

/**
 * Chat's New chat: choose `to` in the host's picker, write `purpose` and ask
 * Contact Chat to start; then answer the host's Send invitation prompt.
 */
async function startChat(
  page: Page,
  options: { to: string; purpose: string; send: boolean },
): Promise<void> {
  await openChats(page);
  await page.click('[aria-label="New chat"]');
  const chooser = page.locator("dialog.prompt", { hasText: "New chat" });
  await chooser.locator("button", { hasText: options.to }).click();
  await page.fill(".chat-new-request textarea", options.purpose);
  await page.click('[data-action="start-chat"]');
  const prompt = page.locator("dialog.prompt", { hasText: "Send invitation" });
  expect(await prompt.innerText()).toContain(options.to);
  await prompt
    .getByRole("button", {
      name: options.send ? "Send" : "Cancel",
      exact: true,
    })
    .click();
  await prompt.waitFor({ state: "detached" });
}

/** Contact Chat's worker in `page`: its stage, what it heard from its host as JSON in order, and its stored record. */
async function workerState(
  page: Page,
): Promise<
  { stage: string; received: string[]; record: string | null } | undefined
> {
  for (const frame of page.frames()) {
    if (!frame.url().includes("__truapi-worker.html")) continue;
    const state = await frame
      .evaluate(() => {
        const worker = (
          window as unknown as {
            __contactChatWorker?: {
              stage: string;
              received: string[];
              record(): string | null;
            };
          }
        ).__contactChatWorker;
        return worker === undefined
          ? undefined
          : {
              stage: worker.stage,
              received: worker.received,
              record: worker.record(),
            };
      })
      .catch(() => undefined);
    if (state !== undefined) return state;
  }
  return undefined;
}

const workerHeard = async (page: Page): Promise<string[]> =>
  (await workerState(page))?.received ?? [];

/** The ids of the sender's pending requests, under Sent in the host's Inbox. */
async function sentIds(page: Page): Promise<string[]> {
  await openInbox(page);
  return page
    .locator("[data-sent]")
    .evaluateAll((rows) =>
      rows.map((row) => (row as HTMLElement).dataset.sent!),
    );
}

/** Read until `read` gives `want`, or fail with the last value. */
async function eventually<T>(read: () => Promise<T>, want: T): Promise<void> {
  const end = Date.now() + STEP_MS;
  let last: T | undefined;
  while (Date.now() < end) {
    last = await read();
    if (last === want) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  expect(last).toBe(want);
}

async function screenshot(page: Page, name: string): Promise<void> {
  const directory = process.env.INVITATION_SCREENSHOTS;
  if (directory)
    await page.screenshot({ path: join(directory, `${name}.png`) });
}

/**
 * What the local delivery holds: the inboxes requests were posted to, or the
 * return routes replies were posted on.
 */
const posts = (page: Page, kind: "request" | "reply") =>
  page.evaluate((kind) => {
    const found = new Set<string>();
    for (const key of Object.keys(localStorage)) {
      const [, rest] = key.split(":dev:delivery:v3:");
      if (rest === undefined) continue;
      const [where, label] = rest.split(":");
      const { route } = JSON.parse(localStorage.getItem(key)!) as {
        route?: string;
      };
      if (kind === "reply" && where === "topic") found.add(`topic:${label}`);
      else if (kind === "reply" && route !== undefined) found.add(route);
      else if (kind === "request" && where === "inbox" && route === undefined)
        found.add(label!);
    }
    return [...found].sort();
  }, kind);

describe("invitations between test users", () => {
  test(
    "connects through a JIT invitation: New chat, send, Accept, an authenticated reply, then Connected",
    async () => {
      const { context, outside, errors, open } = await session();
      // From the first page on: a clock installed under a running light client breaks its timers.
      await context.clock.install();
      try {
        const alice = await open();
        await actAs(alice, "alice");
        const bob = await open();
        await actAs(bob, "bob");
        const carol = await open();
        await actAs(carol, "carol");

        // A test user tab is marked and signed in to the core as that user
        // with no wallet, and opens nothing: Chat shows, and Browse its landing.
        expect(await alice.locator("#test-user-marker").innerText()).toBe(
          "Invitation demo · Alice",
        );
        expect(await alice.locator("#session").innerText()).toContain(
          "Signed in as Alice (invitation demo)",
        );
        expect(await alice.getAttribute("body", "data-surface")).toBe("chat");
        await alice.click(dockTab("browse"));
        await eventually(
          () => alice.getAttribute("body", "data-surface"),
          "browse",
        );
        expect(await alice.locator("#product-slot iframe").count()).toBe(0);
        expect(await alice.inputValue("#invitation-demo")).toBe("alice");

        // The host's own picker chooses; Cancel on the Send invitation prompt
        // sends nothing, and the product says so.
        await startChat(alice, {
          to: "Bob",
          purpose: "Shall we talk?",
          send: false,
        });
        await systemLine(alice, "Your chat request was not sent").waitFor();
        expect(await posts(alice, "request")).toEqual([]);

        await startChat(alice, {
          to: "Bob",
          purpose: "Let's chat in Contact Chat.",
          send: true,
        });
        await systemLine(alice, "You sent a chat request").waitFor();
        const [first] = await sentIds(alice);
        expect(first).toBeDefined();
        await eventually(() => badge(bob), 1);
        expect(await badge(carol)).toBe(0);
        expect(await posts(alice, "request")).toHaveLength(1);
        await screenshot(bob, "bob-badge");

        // Bob sees Alice as his host names her. Seeing it sends nothing.
        await openInbox(bob);
        const row = bob.locator(".notification-row").first();
        await eventually(() => row.getAttribute("data-seen"), "true");
        expect(await row.getAttribute("data-status")).toBe("pending");
        const text = await row.innerText();
        expect(text).toContain("Alice");
        expect(text).toContain("Let's chat in Contact Chat.");
        expect(text).toContain("Chat invitation · Contact Chat");
        const bobRequest = (await row.getAttribute("data-notification"))!;
        await screenshot(bob, "bob-notifications");

        // Accept: Bob's Contact Chat checks the payload and replies through
        // its host, which posts the sealed reply on the request's return
        // route. Alice's host authenticates it, her product checks it, and only
        // then is it Connected.
        await row.locator('[data-action="notification-accept"]').click();
        await openChats(bob);
        await room(bob, "Alice").click();
        await systemLine(bob, "You accepted the chat request").waitFor();
        await openChats(alice);
        await room(alice, "Bob").click();
        await systemLine(alice, "They accepted your chat request").waitFor();
        await eventually(async () => (await sentIds(alice)).length, 0);
        const aliceLog = (await alice.locator("#log").textContent())!;
        expect(aliceLog).toContain("authenticated a reply from Bob");
        expect(aliceLog).toContain("reposting stopped. Nothing is sent.");
        expect(await bob.locator("#log").textContent()).toContain(
          "the host sealed it for Alice and posted it on the request's return route",
        );
        expect(await posts(alice, "reply")).toHaveLength(1);
        await screenshot(alice, "alice-connected");

        // Carol dismisses hers: it leaves her list, nothing goes back, and
        // Alice's request still waits until she cancels it under Sent in her
        // host's Inbox.
        await startChat(alice, {
          to: "Carol",
          purpose: "Join my reading group.",
          send: true,
        });
        await eventually(() => badge(carol), 1);
        const [second] = await sentIds(alice);
        await openInbox(carol);
        await carol
          .locator('.notification-row [data-action="notification-dismiss"]')
          .click();
        await carol.locator(".notification-row").waitFor({ state: "detached" });
        await carol.waitForTimeout(300);
        expect(await sentIds(alice)).toEqual([second!]);
        // Requests went to Bob and Carol; the one reply is Bob's, and Carol
        // posted nothing.
        expect(await posts(alice, "request")).toHaveLength(2);
        expect(await posts(alice, "reply")).toHaveLength(1);
        await alice
          .locator(`[data-sent="${second}"] [data-action="sent-cancel"]`)
          .click();
        await alice.locator(`[data-sent="${second}"]`).waitFor({
          state: "detached",
        });
        await openChats(alice);
        await room(alice, "Carol").click();
        await systemLine(alice, "You cancelled your chat request").waitFor();
        expect(await posts(alice, "reply")).toHaveLength(1);

        // Bob's accepted row goes to his Contact Chat as an open intent, and
        // the host takes the product's answer to it.
        await openInbox(bob);
        await bob
          .locator(
            `.notification-row[data-notification="${bobRequest}"] [data-action="notification-open"]`,
          )
          .click();
        await eventually(async () => {
          const heard = await workerHeard(bob);
          const intent = heard.findIndex(
            (line) =>
              line.includes('"openIntent"') && line.includes(bobRequest),
          );
          return (
            intent >= 0 &&
            heard
              .slice(intent)
              .includes(JSON.stringify({ result: { ok: true } }))
          );
        }, true);

        // A reload keeps the test user and both lists.
        await bob.reload();
        await ready(bob);
        await openInbox(bob);
        expect(
          await bob
            .locator('[data-group="earlier"] .notification-row')
            .evaluateAll((rows) =>
              rows.map((node) => (node as HTMLElement).dataset.status),
            ),
        ).toEqual(["accepted"]);
        expect(await badge(bob)).toBe(0);

        // Another network shows none of it.
        await setMenu(alice, true);
        await Promise.all([
          alice.waitForEvent("load"),
          alice.selectOption("#network", "previewnet"),
        ]);
        await ready(alice);
        expect(await sentIds(alice)).toEqual([]);
        expect(await alice.locator(".notification-row").count()).toBe(0);
        // Contact Chat runs there with none of the conversations it keeps on Paseo.
        await eventually(
          async () => (await workerState(alice))?.stage,
          "running",
        );
        expect((await workerState(alice))?.record).toBeNull();
        await openChats(alice);
        expect(
          await alice.locator('[data-conversation^="chat-v2:"]').count(),
        ).toBe(0);

        const keys = await alice.evaluate(() =>
          Object.keys(localStorage)
            .filter((key) => key.includes(":test-user:"))
            .sort(),
        );
        expect(keys).toEqual([
          "truapi-web-signing-host:network:paseo:test-user:alice:contact-chat-outbox:v2",
          "truapi-web-signing-host:network:paseo:test-user:bob:contact-chat-received:v2",
          "truapi-web-signing-host:network:paseo:test-user:bob:notifications:v2",
          "truapi-web-signing-host:network:paseo:test-user:carol:contact-chat-received:v2",
          "truapi-web-signing-host:network:paseo:test-user:carol:notifications:v2",
          // Contact Chat's own record and read marks, in its TrUAPI local storage.
          "truapi-web-signing-host:product:test-user:alice:truapi:product-storage:v1:18:contact-chat.paseo:contact-chat:read:v1",
          "truapi-web-signing-host:product:test-user:alice:truapi:product-storage:v1:18:contact-chat.paseo:contact-chat:record:v3",
          "truapi-web-signing-host:product:test-user:bob:truapi:product-storage:v1:18:contact-chat.paseo:contact-chat:read:v1",
          "truapi-web-signing-host:product:test-user:bob:truapi:product-storage:v1:18:contact-chat.paseo:contact-chat:record:v3",
        ]);
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a test account signs in as any wallet does, and the invitation demo stays apart from it",
    async () => {
      const { context, outside, errors, open } = await session();
      try {
        const page = await open();
        const marker = page.locator("#test-user-marker");
        const sessionLine = page.locator("#session");
        const signedIn = (text: string) =>
          sessionLine.filter({ hasText: text }).waitFor({ state: "attached" });
        /** What Chat's empty screen says, or null when it shows none. */
        const chatNote = async () => {
          await setMenu(page, false);
          await page.click(dockTab("chat"));
          await page.click('#chat-tabs [data-chat-tab="chats"]');
          const title = page.locator("#chat-screen .chat-empty-title");
          return (await title.count()) === 0
            ? null
            : await title.first().innerText();
        };
        /** Chat runs for the session: it no longer asks to sign in. */
        const chatRuns = () =>
          eventually(
            async () => (await chatNote()) !== "Sign in to chat",
            true,
          );
        const rootKey = async () =>
          (await sessionLine.innerText()).split(" · ")[1];
        const workerStatus = async () => {
          await setMenu(page, true, "product");
          return page.locator("#worker-status").innerText();
        };

        // Signed out: the picker offers the shared test accounts as wallets,
        // and chooses none of them for the user.
        expect(await chatNote()).toBe("Sign in to chat");
        await setMenu(page, true);
        expect(
          await page
            .locator('#wallet optgroup[label="Shared test accounts"] option')
            .allInnerTexts(),
        ).toEqual([
          "Alice (test account)",
          "Bob (test account)",
          "Carol (test account)",
        ]);
        expect({
          value: await page.inputValue("#wallet"),
          shown: await page.locator("#wallet option:checked").innerText(),
          signIn: await page.locator("#sign-in").isDisabled(),
        }).toEqual({ value: "", shown: "Choose an account", signIn: true });
        const about = page.locator("#test-accounts-about");
        expect(await about.isHidden()).toBe(true);
        await page.selectOption("#wallet", "test-account:alice");
        expect(await about.isHidden()).toBe(false);
        expect(await about.locator("summary").innerText()).toBe(
          "Shared test account: anyone can act as it",
        );

        // Alice signs in without a reload and runs as a wallet: Chat, workers
        // and the Account status, with Sign out and no Forget.
        await page.selectOption("#wallet", "test-account:alice");
        await page.click("#sign-in");
        await signedIn("Signed in as Alice (test account)");
        expect(await marker.isHidden()).toBe(true);
        await chatRuns();
        expect(await chatNote()).not.toBe("Sign in to chat");
        expect(await workerStatus()).not.toContain("Sign in with a wallet");
        await setMenu(page, true);
        expect(await page.locator("#sign-out").isVisible()).toBe(true);
        expect(await page.locator("#forget").isHidden()).toBe(true);
        expect(await page.locator("#account-status").isHidden()).toBe(false);
        const aliceKey = await rootKey();
        // A reload keeps it.
        await page.reload();
        await ready(page);
        await signedIn("Signed in as Alice (test account)");
        await chatRuns();

        // The same phrase imported is the same account on the same path.
        await setMenu(page, true);
        await page
          .locator("#import")
          .evaluate((details) => ((details as HTMLDetailsElement).open = true));
        await page.fill("#wallet-name", "Imported Alice");
        await page.fill(
          "#mnemonic",
          "jelly better achieve collect unaware mountain thought cargo oxygen act hood bridge",
        );
        await page.click("#add-wallet");
        await signedIn("Signed in as Imported Alice");
        expect(await rootKey()).toBe(aliceKey);
        await chatRuns();
        expect(await workerStatus()).not.toContain("Sign in with a wallet");
        const walletId = await page.inputValue("#wallet");
        expect(await about.isHidden()).toBe(true);
        const saved = await page.evaluate(() =>
          localStorage.getItem("truapi-web-signing-host:wallets:v1"),
        );
        await menuAction(page, "notifications-demo-seed");
        await eventually(() => badge(page), 4);

        // The invitation demo is chosen in More and is not an account: its
        // made-up Bob gets none of the wallet's workers or Inbox. Chat shows
        // only the demo's own chat worker, Contact Chat.
        await actAs(page, "bob");
        await signedIn("Signed in as Bob (invitation demo)");
        expect(await marker.innerText()).toBe("Invitation demo · Bob");
        expect(await chatNote()).not.toBe("Sign in to chat");
        const demoWorkers = await workerStatus();
        expect(demoWorkers).not.toContain("Sign in with a wallet");
        expect(await page.locator("#worker-list").innerText()).toContain(
          "contact-chat.paseo",
        );
        expect(await badge(page)).toBe(0);
        await setMenu(page, true);
        expect(
          await page
            .locator("#wallet option")
            .evaluateAll((options) =>
              options.map((option) => (option as HTMLOptionElement).value),
            ),
        ).toEqual([
          "",
          walletId,
          "test-account:alice",
          "test-account:bob",
          "test-account:carol",
        ]);

        // Clearing Bob's invitations asks first, and Cancel keeps them.
        await page.evaluate(() =>
          localStorage.setItem(
            "truapi-web-signing-host:network:paseo:test-user:bob:contact-chat-outbox:v2",
            "[]",
          ),
        );
        const bobKeys = () =>
          page.evaluate(() =>
            Object.keys(localStorage).filter((key) =>
              key.includes(":test-user:bob:"),
            ),
          );
        await menuAction(page, "clear-test-invitations");
        const clear = page.locator("dialog.prompt", {
          hasText: "Clear Bob's invitations?",
        });
        await clear.getByRole("button", { name: "Cancel" }).click();
        expect(await bobKeys()).toContain(
          "truapi-web-signing-host:network:paseo:test-user:bob:contact-chat-outbox:v2",
        );
        await menuAction(page, "clear-test-invitations");
        await Promise.all([
          page.waitForEvent("load"),
          clear.getByRole("button", { name: "Clear", exact: true }).click(),
        ]);
        await ready(page);
        await signedIn("Signed in as Bob (invitation demo)");
        expect(
          (await bobKeys()).filter((key) => !key.includes(":product:")),
        ).toEqual([]);

        // Sign out leaves the demo signed out, with no wallet signed in for it.
        await setMenu(page, true);
        await Promise.all([page.waitForEvent("load"), page.click("#sign-out")]);
        await ready(page);
        expect(await sessionLine.innerText()).toBe("Signed out");
        expect(await marker.isHidden()).toBe(true);
        expect(await chatNote()).toBe("Sign in to chat");

        // The imported wallet comes back as it was, with its Inbox.
        await setMenu(page, true);
        await page.selectOption("#wallet", walletId);
        await page.click("#sign-in");
        await signedIn("Signed in as Imported Alice");
        expect(
          await page.evaluate(() =>
            localStorage.getItem("truapi-web-signing-host:wallets:v1"),
          ),
        ).toBe(saved);
        await eventually(() => badge(page), 4);
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test(
    "fits a phone: the marker, the list and the request banner, with nothing scrolling sideways",
    async () => {
      const { context, open } = await session({ width: 390, height: 844 });
      try {
        const alice = await open();
        await actAs(alice, "alice");
        const bob = await open();
        await actAs(bob, "bob");
        await startChat(alice, {
          to: "Bob",
          purpose:
            "We would like to start a chat about the reading circle schedule and the next three meetings.",
          send: true,
        });
        await eventually(() => badge(bob), 1);
        const sideways = () =>
          bob.evaluate(
            () => document.documentElement.scrollWidth - window.innerWidth,
          );
        expect(await sideways()).toBeLessThanOrEqual(0);
        await screenshot(bob, "phone-bob-badge");
        await openInbox(bob);
        const accept = await bob
          .locator('[data-action="notification-accept"]')
          .boundingBox();
        expect(accept!.x + accept!.width).toBeLessThanOrEqual(390);
        expect(accept!.height).toBeGreaterThanOrEqual(40);
        expect(await sideways()).toBeLessThanOrEqual(0);
        await screenshot(bob, "phone-bob-request");
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
