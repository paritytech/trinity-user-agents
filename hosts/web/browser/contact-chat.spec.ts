/**
 * Contact Chat between two test users of the invitation demo, each in their
 * own tab of the normal host. No wallet signs in, every socket is closed and
 * every request outside this host and its loopback fixture product is
 * refused, so no chain, backend or peer is reached. It needs a Chromium: playwright-core finds the one it installs, or
 * set CHROMIUM_PATH.
 *
 * What runs for real: the dev server's archive of
 * `apps/contact-chat/worker.ts`, the archive check, the
 * ordinary worker loader and frame, the product's TrUAPI storage, the core's
 * contact handles (the ones `contacts.pick` gives, which the host names
 * through the handles the core mints for it), the request service, the v2
 * draft bridge, the host's Chat screen and Inbox, and the host proofs, made
 * and checked with the keys each test user's own phrase derives after the
 * host checked them against the core's session. The chat keys a host reads
 * for its contacts come from a fixture directory, not the chain; the delivery
 * is the local simulated one. Contact Chat posts text through its TrUAPI
 * Statement Store as its own account at each conversation's index, and the
 * local delivery gives such an account no allowance, so a message here fails
 * and says why; no statement leaves the tab. A second
 * product, `fixtures/products/contacts-product.ts` served by URL on a loopback
 * port, asks `contacts.pick` for the same people.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createServer, type Server } from "node:http";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  type RunningHost,
  buildFixture,
  closeTools,
  freePort,
  openTools,
  showAddress,
  startHost,
  testBrowser,
  useInvitationDemo,
  probeChatWorkers,
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
/** Another product of the same users: `contacts-product.ts`, served by URL on a port of its own. */
let other: { url: string; server: Server };

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
  const code = await buildFixture("contacts-product.ts");
  const port = await freePort();
  const server = createServer((request, response) => {
    if (request.url === "/index.js") {
      response.writeHead(200, { "content-type": "text/javascript" });
      return void response.end(code);
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(
      '<!doctype html><meta charset="utf-8"><title>contacts product</title><script type="module" src="/index.js"></script>',
    );
  });
  await new Promise<void>((resolve) =>
    server.listen(port, "127.0.0.1", () => resolve()),
  );
  other = { url: `http://localhost:${port}/`, server };
}, STEP_MS);

afterAll(async () => {
  other?.server.close();
  await host?.close();
});

/** A browser context with every socket closed and every request outside this host and the other product refused. */
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
    if (url.startsWith(host.origin) || url.startsWith(other.url))
      return route.fallback();
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
  __contactChatWorker?: {
    stage: string;
    reason?: string;
    errors: string[];
    received: string[];
    starts: string[];
    record(): string | null;
    pickAndStart(
      welcome: string,
    ): Promise<{ roomId: string } | { outcome: string }>;
    cancel(roomId: string): Promise<void>;
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

const picker = (page: Page) =>
  page.locator("dialog.prompt", { hasText: "Choose a contact" });

/** Open the other product by its address, ask `contacts.pick` there, choose `name`, and read the handle it got. */
async function pickInOtherProduct(page: Page, name: string): Promise<string> {
  await showAddress(page);
  await page.fill("#address", other.url);
  await page.press("#address", "Enter");
  let frame: Frame | undefined;
  const deadline = Date.now() + STEP_MS;
  while (frame === undefined && Date.now() < deadline) {
    for (const each of page.frames())
      if (
        each.url().startsWith(other.url) &&
        (await each.evaluate(() => "__contacts" in window).catch(() => false))
      )
        frame = each;
    if (frame === undefined) await page.waitForTimeout(200);
  }
  if (frame === undefined) throw new Error("the other product did not start");
  const picked = frame.evaluate(() =>
    (
      window as unknown as {
        __contacts: {
          pick(): Promise<{
            outcome?: { value?: { handle: { bytes: string } } };
          }>;
        };
      }
    ).__contacts.pick(),
  );
  await picker(page)
    .getByRole("button", { name: new RegExp(`^${name}\\b`) })
    .click();
  await picker(page).waitFor({ state: "detached" });
  return (await picked).outcome!.value!.handle.bytes;
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
    "invite by contact handle, accept in the host, and connect; the host names everyone",
    async () => {
      const { context, outside, errors, open } = await offline();
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

        // Alice's host authenticated the reply, her worker checked it: they
        // are connected, and told the text is not encrypted. The local
        // delivery gives the conversation's account no allowance, so a
        // message fails and says why.
        await alice
          .locator(".chat-system", {
            hasText: "They accepted your chat request",
          })
          .waitFor();
        await alice
          .locator(".chat-row-preview", {
            hasText: "Connected · messages are not encrypted",
          })
          .first()
          .waitFor();
        await alice.fill(".chat-composer-input", "Hello there");
        await alice.press(".chat-composer-input", "Enter");
        await alice.locator(".chat-failed").waitFor();
        await alice
          .locator(".chat-system", {
            hasText: "gives conversation accounts no Statement Store allowance",
          })
          .waitFor();
        await screenshot(alice, "contact-chat-conversation");

        // The inspector's Chat view joins what Contact Chat reported of the
        // conversation with what Alice's host holds of its request: the
        // allowance was refused, so nothing was submitted.
        await openTools(alice, "inspector");
        await alice.click('[data-inspector-view="chat"]');
        const card = alice.locator("#chat-inspector .si-card", {
          hasText: "contact-chat/simple-text/1",
        });
        await card.locator(".si-badge", { hasText: "connected" }).waitFor();
        // The worker reports at most twice a second: wait for the failed send.
        await card.locator(".si-facts dd", { hasText: "1 failed" }).waitFor();
        const facts = await card.locator(".si-facts").first().innerText();
        expect(facts).toContain("sender · completed");
        expect(facts).toContain(
          "refused: This host gives conversation accounts no Statement Store allowance.",
        );
        // No claim or allowance ties the account to Contact Chat here: it is reported only.
        expect(facts).toContain("reported only");
        expect(facts).toMatch(/0 sent · 0 received · 0 duplicates · 1 failed/);
        await screenshot(alice, "contact-chat-statements");
        await closeTools(alice);

        // A reload keeps both conversations, named by each host.
        await bob.reload();
        await ready(bob);
        await workerFrame(bob);
        await chat(bob);
        await row(bob, "Alice").click();
        await bob
          .locator(".chat-system", { hasText: "You accepted the chat request" })
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
          // Each kept its session, and the index and public key of its own
          // product account for the conversation, which the core reported.
          expect(visible).toContain('"session":{"listen":"0x');
          expect(visible).toMatch(/"index":"0x[0-9a-f]{64}"/);
          expect(visible).toMatch(/"account":"0x[0-9a-f]{64}"/);
        }
        // What the worker could not do: the refused send, and reading a
        // Statement Store this offline tab cannot reach.
        const workerErrors = await aliceWorker.evaluate(
          () => (window as unknown as WorkerWindow).__contactChatWorker!.errors,
        );
        expect(
          workerErrors.filter((error) => !error.startsWith("read: ")),
        ).toEqual([
          "send: This host gives conversation accounts no Statement Store allowance.",
        ]);

        // The handle each worker holds is the core's: the one `contacts.pick`
        // gives another product of the same user for the same person.
        const aliceHolds = await heldHandles(aliceWorker);
        const bobHolds = await heldHandles(bobWorker);
        expect([
          aliceHolds,
          bobHolds,
          [await pickInOtherProduct(alice, "Bob")],
          [await pickInOtherProduct(bob, "Alice")],
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

  test(
    "the product picks a contact with contacts.pick, requests a chat with that handle through its host, and cancels it itself",
    async () => {
      const { context, outside, errors, open } = await offline();
      try {
        const alice = await open();
        await actAs(alice, "alice");
        const carol = await open();
        await actAs(carol, "carol");
        const aliceWorker = await workerFrame(alice);
        await workerFrame(carol);

        // The product asks the core for a contact; the host's picker names it as the asker.
        const started = aliceWorker.evaluate(() =>
          (window as unknown as WorkerWindow).__contactChatWorker!.pickAndStart(
            "Picked in the product",
          ),
        );
        await picker(alice).waitFor();
        expect(await picker(alice).innerText()).toContain(PRODUCT);
        await picker(alice)
          .getByRole("button", { name: /^Carol\b/ })
          .click();
        // The product's request goes to the picked handle; the host names
        // the person and asks before it leaves.
        const prompt = alice.locator("dialog.prompt", {
          hasText: "Send invitation",
        });
        expect(await prompt.innerText()).toContain("Carol");
        await prompt.getByRole("button", { name: "Send", exact: true }).click();
        const result = await started;
        if (!("roomId" in result)) throw new Error(JSON.stringify(result));
        const record = () =>
          aliceWorker.evaluate(() =>
            JSON.parse(
              (
                window as unknown as WorkerWindow
              ).__contactChatWorker!.record()!,
            ),
          ) as Promise<{
            conversations: { id: string; contact: string; status: string }[];
          }>;
        const conversation = async () =>
          (await record()).conversations.find(
            (entry) => entry.id === result.roomId,
          )!;
        /** The conversation once its status is `status`, or the last one read. */
        const reaching = async (status: string) => {
          const until = Date.now() + STEP_MS;
          let last = await conversation();
          while (last.status !== status && Date.now() < until) {
            await alice.waitForTimeout(100);
            last = await conversation();
          }
          return last;
        };
        const sent = await reaching("waiting");
        expect(sent.status).toBe("waiting");
        expect(sent.contact).toBe(await pickInOtherProduct(alice, "Carol"));

        // Carol's host authenticated it and names Alice.
        await chat(carol);
        await carol.click('#chat-tabs [data-chat-tab="inbox"]');
        const request = carol.locator(".notification-row").first();
        await request.waitFor();
        expect(await request.innerText()).toContain("Alice");
        expect(await request.innerText()).toContain("Picked in the product");

        // The product cancels its own request; the host stops it, and Carol is not told.
        await aliceWorker.evaluate(
          (roomId) =>
            (window as unknown as WorkerWindow).__contactChatWorker!.cancel(
              roomId,
            ),
          result.roomId,
        );
        expect((await reaching("cancelled")).status).toBe("cancelled");
        await chat(alice);
        await alice.click('#chat-tabs [data-chat-tab="inbox"]');
        expect(await alice.locator("[data-sent]").count()).toBe(0);
        expect(await alice.locator("#log").textContent()).toContain(
          "cancelled; reposting stopped. Nothing is sent.",
        );
        await carol.waitForTimeout(300);
        expect(
          await carol
            .locator('.notification-row[data-status="pending"]')
            .count(),
        ).toBe(1);
        expect(outside).toEqual([]);
        expect(errors).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
