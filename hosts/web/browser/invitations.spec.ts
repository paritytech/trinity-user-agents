/**
 * Headless-browser test of invitations between two test users, each in its
 * own tab of the normal host. No wallet signs in, every socket is closed and
 * every request outside this host is refused, so no chain, backend or peer is
 * reached. It needs a Chromium: playwright-core finds the one it installs, or
 * set CHROMIUM_PATH.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import type {
  Browser,
  BrowserContext,
  FrameLocator,
  Locator,
  Page,
} from "playwright-core";
import {
  type RunningHost,
  startHost,
  testBrowser,
  closeTools,
  openTools,
  type DevTab,
} from "./fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 240_000;
/** A BIP-39 test vector phrase. It holds nothing. */
const PHRASE =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";

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

/** Menu → Developer → Test user. The tab reloads as that user. */
async function actAs(page: Page, user: string): Promise<void> {
  await setMenu(page, true, "more");
  await Promise.all([
    page.waitForEvent("load"),
    page.selectOption("#test-user", user),
  ]);
  await ready(page);
  await setMenu(page, false);
}

async function menuAction(page: Page, action: string): Promise<void> {
  await setMenu(page, true, "more");
  await page.click(`[data-action="${action}"]`);
  await setMenu(page, false);
}

const exampleChat = (page: Page): FrameLocator =>
  page.frameLocator(".test-product iframe");

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
  await page.click(inboxTab);
}

/** Choose a contact for the invitation through the host's picker. */
async function chooseContact(page: Page, name: string): Promise<string> {
  const product = exampleChat(page);
  await product.locator('[data-action="choose-contact"]').click();
  const picker = page.locator("dialog.prompt", { hasText: "Choose a contact" });
  await picker.getByRole("button", { name: new RegExp(`^${name}\\b`) }).click();
  const chosen = product.locator(
    '[data-action="choose-contact"][data-contact^="0x"]',
  );
  await chosen.waitFor();
  return (await chosen.getAttribute("data-contact"))!;
}

/** Invite from Example Chat: choose `to`, then confirm the host's Send invitation prompt. */
async function invite(
  page: Page,
  options: { purpose: string; expiry: string; to: string },
): Promise<string> {
  const product = exampleChat(page);
  const rows = product.locator('[data-list="sent"] .row');
  const before = await rows.count();
  await chooseContact(page, options.to);
  await product.locator('textarea[name="purpose"]').fill(options.purpose);
  await product
    .locator('select[name="expiry"]')
    .selectOption({ label: `Expires in ${options.expiry}` });
  await product.locator('[data-action="invite"]').click();
  const prompt = page.locator("dialog.prompt", { hasText: "Send invitation" });
  expect(await prompt.innerText()).toContain(options.to);
  await prompt.getByRole("button", { name: "Send", exact: true }).click();
  await eventually(() => rows.count(), before + 1);
  return (await rows.first().getAttribute("data-id"))!;
}

const sentRow = (page: Page, id: string): Locator =>
  exampleChat(page).locator(`[data-list="sent"] .row[data-id="${id}"]`);

const sentStatus = (page: Page, id: string) =>
  sentRow(page, id).getAttribute("data-status");

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
    "connects through a JIT invitation: pick, send, Accept, an authenticated reply, then Connected",
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

        // A test user tab is marked, signed in to the core as that user with no
        // wallet, and shows Example Chat in Browse.
        expect(await alice.locator("#test-user-marker").innerText()).toBe(
          "Test user · Alice",
        );
        expect(await alice.locator("#session").innerText()).toContain(
          "Signed in as Alice",
        );
        expect(await alice.getAttribute("body", "data-surface")).toBe(
          "test-product",
        );
        expect(await alice.locator("#product-slot #test-user").count()).toBe(0);
        expect(await alice.locator("#devtools #test-user").count()).toBe(1);
        await screenshot(alice, "alice-example-chat");

        // The host's own picker chooses; the product gets an opaque reference.
        // Cancel on the Send invitation prompt sends nothing.
        await chooseContact(alice, "Bob");
        await exampleChat(alice).locator('[data-action="invite"]').click();
        const prompt = alice.locator("dialog.prompt", {
          hasText: "Send invitation",
        });
        await prompt.getByRole("button", { name: "Cancel" }).click();
        await exampleChat(alice)
          .locator('[data-card="invite"] .note', { hasText: "Not sent." })
          .waitFor();
        expect(await posts(alice, "request")).toEqual([]);

        const first = await invite(alice, {
          purpose: "Let's chat in Example Chat.",
          expiry: "1 hour",
          to: "Bob",
        });
        expect(await sentStatus(alice, first)).toBe("waiting");
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
        expect(text).toContain("Let's chat in Example Chat.");
        expect(text).toContain("Chat invitation · Example Chat");
        const bobRequest = (await row.getAttribute("data-notification"))!;
        await screenshot(bob, "bob-notifications");

        // Accept: Bob's Example Chat checks the payload and replies through
        // its host, which posts the sealed reply on the request's return
        // route. Alice's host authenticates it, her product checks it, and only
        // then is it Connected.
        await row.locator('[data-action="notification-accept"]').click();
        await eventually(
          () => bob.getAttribute("body", "data-surface"),
          "test-product",
        );
        const acceptedRow = exampleChat(bob).locator(
          `[data-list="accepted"] .row[data-id="${bobRequest}"]`,
        );
        await acceptedRow.waitFor();
        await eventually(() => sentStatus(alice, first), "completed");
        expect(await sentRow(alice, first).innerText()).toContain("Connected");
        const aliceLog = (await alice.locator("#log").textContent())!;
        expect(aliceLog).toContain("authenticated a reply from Bob");
        expect(aliceLog).toContain("reposting stopped. Nothing is sent.");
        expect(await bob.locator("#log").textContent()).toContain(
          "the host sealed it for Alice and posted it on the request's return route",
        );
        expect(await posts(alice, "reply")).toHaveLength(1);
        // Neither product shows who the other person is.
        expect(
          await exampleChat(alice).locator("main").innerText(),
        ).not.toContain("Bob");
        expect(
          await exampleChat(bob).locator("main").innerText(),
        ).not.toContain("Alice");
        await screenshot(alice, "alice-connected");

        // Carol dismisses hers: it leaves her list, nothing goes back, and
        // Alice's invitation still waits until she cancels it.
        const second = await invite(alice, {
          purpose: "Join my reading group.",
          expiry: "1 day",
          to: "Carol",
        });
        await eventually(() => badge(carol), 1);
        await openInbox(carol);
        await carol
          .locator('.notification-row [data-action="notification-dismiss"]')
          .click();
        await carol.locator(".notification-row").waitFor({ state: "detached" });
        await carol.waitForTimeout(300);
        expect(await sentStatus(alice, second)).toBe("waiting");
        // Requests went to Bob and Carol; the one reply is Bob's, and Carol
        // posted nothing.
        expect(await posts(alice, "request")).toHaveLength(2);
        expect(await posts(alice, "reply")).toHaveLength(1);
        await sentRow(alice, second)
          .locator('[data-action="cancel-invitation"]')
          .click();
        await eventually(() => sentStatus(alice, second), "cancelled");

        // Expired: no completion by its expiry; Bob's row loses its actions.
        await openInbox(bob);
        const third = await invite(alice, {
          purpose: "Quick question before the meeting.",
          expiry: "2 minutes",
          to: "Bob",
        });
        await eventually(() => bob.locator(".notification-row").count(), 2);
        await bob
          .locator(
            '.notification-row[data-status="pending"] [data-action="notification-accept"]',
          )
          .waitFor();
        await context.clock.fastForward("03:00");
        await eventually(() => sentStatus(alice, third), "expired");
        await eventually(
          () => bob.locator('.notification-row[data-status="expired"]').count(),
          1,
        );
        expect(
          await bob
            .locator('.notification-row[data-status="expired"] [data-action]')
            .count(),
        ).toBe(0);

        // Restarting Example Chat leaves one frame; its rows come back.
        await menuAction(alice, "restart-example-chat");
        await eventually(
          () => exampleChat(alice).locator('[data-list="sent"] .row').count(),
          3,
        );
        expect(await alice.locator(".test-product iframe").count()).toBe(1);
        expect(await sentStatus(alice, first)).toBe("completed");
        expect(await sentStatus(alice, second)).toBe("cancelled");

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
        ).toEqual(["expired", "accepted"]);
        expect(await badge(bob)).toBe(0);

        // Another network shows none of it.
        await setMenu(alice, true);
        await Promise.all([
          alice.waitForEvent("load"),
          alice.selectOption("#network", "previewnet"),
        ]);
        await ready(alice);
        await exampleChat(alice).locator('[data-action="invite"]').waitFor();
        expect(
          await exampleChat(alice).locator('[data-list="sent"] .row').count(),
        ).toBe(0);

        const keys = await alice.evaluate(() =>
          Object.keys(localStorage)
            .filter((key) => key.includes(":test-user:"))
            .sort(),
        );
        expect(keys).toEqual([
          "truapi-web-signing-host:network:paseo:test-user:alice:outbox:v2",
          "truapi-web-signing-host:network:paseo:test-user:bob:notifications:v1",
          "truapi-web-signing-host:network:paseo:test-user:bob:received:v2",
          "truapi-web-signing-host:network:paseo:test-user:carol:notifications:v1",
          "truapi-web-signing-host:network:paseo:test-user:carol:received:v2",
          // Example Chat's own secrets and replies, in its TrUAPI local storage.
          "truapi-web-signing-host:product:test-user:alice:truapi:product-storage:v1:18:example-chat.paseo:example-chat:invitations:v2",
          "truapi-web-signing-host:product:test-user:bob:truapi:product-storage:v1:18:example-chat.paseo:example-chat:invitations:v2",
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
    "acting as a test user leaves the saved wallet and its data alone",
    async () => {
      const { context, outside, open } = await session();
      try {
        const page = await open();
        await setMenu(page, true);
        await page
          .locator("#import")
          .evaluate((details) => ((details as HTMLDetailsElement).open = true));
        await page.fill("#mnemonic", PHRASE);
        await page.click("#add-wallet");
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await menuAction(page, "notifications-demo-seed");
        await eventually(() => badge(page), 4);

        // Alice's session is the test phrase's, not the wallet's.
        const walletSession = await page.locator("#session").innerText();
        await actAs(page, "alice");
        const aliceSession = await page.locator("#session").innerText();
        expect(aliceSession).toContain("Signed in as Alice");
        expect(aliceSession.split(" · ")[1]).not.toBe(
          walletSession.split(" · ")[1],
        );
        expect(await badge(page)).toBe(0);

        await actAs(page, "");
        expect(await page.locator("#test-user-marker").isHidden()).toBe(true);
        expect(await page.locator("#wallet option").count()).toBe(1);
        await setMenu(page, true);
        await page.click("#sign-in");
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await eventually(() => badge(page), 4);
        expect(outside).toEqual([]);
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
        await invite(alice, {
          purpose:
            "We would like to start a chat about the reading circle schedule and the next three meetings.",
          expiry: "1 day",
          to: "Bob",
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
