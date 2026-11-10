/**
 * Kettle Chat's log recovery over iroh, between two test users of the
 * invitation demo in two tabs of the normal host, against an iroh relay this
 * test runs on loopback. Every other socket is closed and every request
 * outside the host and the relay is refused: no chain, backend, public relay
 * or peer is reached.
 *
 * Needs Kettle's WebAssembly (`npm run build:kettle`) and an `iroh-relay`
 * 1.4.1 binary with the `server` feature, at `IROH_RELAY_BIN` or on PATH
 * (`cargo install iroh-relay@1.4.1 --features server --locked`). Without
 * either it is skipped.
 *
 * What runs for real: the dev server's archive of Kettle Chat's worker, the
 * worker loader and frame, Kettle's engine with its log transfer, iroh's
 * browser endpoint, the relay, the core's product accounts and storage, the
 * request service and the Chat screen. Simulated: the People node's
 * statement methods (`fixtures/statements/local-statement-node.ts`, which
 * keeps one outbox per account and channel), allowances, request delivery.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
  freePort,
  openTools,
  startHost,
  testBrowser,
  useChatProducts,
  useInvitationDemo,
  probeChatWorkers,
} from "./support/fixtures.js";
import { KETTLE_WASM, KETTLE_WASM_PACKAGE } from "../apps/kettle-chat/route.js";
import { NETWORKS } from "../src/platform/network-config.js";

const STEP_MS = 60_000;
const TEST_MS = 300_000;
const PRODUCT = "kettle-chat.paseo";
/** Messages Alice sends while Bob is away: more than one outbox holds. */
const MISSED = 40;

const RELAY_BIN =
  process.env.IROH_RELAY_BIN ??
  (spawnSync("which", ["iroh-relay"]).stdout.toString().trim() || null);
const built = existsSync(
  new URL(`../${KETTLE_WASM_PACKAGE}/${KETTLE_WASM}`, import.meta.url),
);
const runs = built && RELAY_BIN !== null && existsSync(RELAY_BIN);

let browser: Browser;
let host: RunningHost;
interface Relay {
  url: string;
  metrics: string;
  process: ChildProcess;
  dir: string;
}
/** The relay the tests configure. */
let relay: Relay;
/** A second relay, named only in a peer's own address: no tab may reach it but that peer's. */
let decoy: Relay;

async function startRelay(): Promise<Relay> {
  const [port, metricsPort] = [await freePort(), await freePort()];
  const dir = mkdtempSync(join(tmpdir(), "kettle-iroh-relay-"));
  const config = join(dir, "relay.toml");
  writeFileSync(
    config,
    [
      "enable_relay = true",
      `http_bind_addr = "127.0.0.1:${port}"`,
      "enable_quic_addr_discovery = false",
      "enable_metrics = true",
      `metrics_bind_addr = "127.0.0.1:${metricsPort}"`,
    ].join("\n"),
  );
  const child = spawn(RELAY_BIN!, ["--dev", "--config-path", config], {
    stdio: "ignore",
  });
  const metrics = `http://127.0.0.1:${metricsPort}/metrics`;
  for (let wait = 0; wait < 100; wait += 1) {
    if (
      await fetch(metrics).then(
        (r) => r.ok,
        () => false,
      )
    )
      return {
        url: `http://127.0.0.1:${port}/`,
        metrics,
        process: child,
        dir,
      };
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  child.kill();
  throw new Error("the iroh relay did not start");
}

/** A relay's connections: accepted, and disconnected since. */
async function relayConnections(
  of: Relay = relay,
): Promise<{ accepts: number; open: number }> {
  const text = await (await fetch(of.metrics)).text();
  const count = (name: string) =>
    Number(text.match(new RegExp(`^${name} (\\d+)`, "m"))?.[1] ?? NaN);
  const accepts = count("relayserver_accepts_total");
  return { accepts, open: accepts - count("relayserver_disconnects_total") };
}

beforeAll(async () => {
  if (!runs) return;
  relay = await startRelay();
  decoy = await startRelay();
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
  for (const each of [relay, decoy]) {
    if (each === undefined) continue;
    const exited = new Promise((resolve) => each.process.once("exit", resolve));
    each.process.kill();
    await exited;
    rmSync(each.dir, { recursive: true, force: true });
  }
});

/** The relays a tab's sockets went to, by page, with the host's own (Vite's) left out. */
type Sockets = Map<Page, string[]>;

/** The hosts of n0's public test relays, as iroh 1.4.1 ships them. */
const PUBLIC_RELAYS = [
  "aps1-1.relay.n0.iroh.link.",
  "euc1-1.relay.n0.iroh.link.",
  "use1-1.relay.n0.iroh.link.",
  "usw1-1.relay.n0.iroh.link.",
];
const isPublicRelay = (host: string) =>
  PUBLIC_RELAYS.map((name) => name.replace(/\.$/, "")).includes(
    host.replace(/\.$/, ""),
  );
/** Only `KETTLE_IROH_PUBLIC=1` lets a test reach the public relays: one short smoke. */
const PUBLIC = process.env.KETTLE_IROH_PUBLIC === "1";

async function offline(options: { publicRelays?: boolean } = {}): Promise<{
  context: BrowserContext;
  outside: string[];
  errors: string[];
  sockets: Sockets;
  /**
   * Every socket any tab of the context asked for, but the dev server's own
   * and the core's chain connections, which this test closes.
   */
  attempted: string[];
  open(): Promise<Page>;
}> {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 860 },
  });
  context.setDefaultTimeout(STEP_MS);
  await probeChatWorkers(context);
  const local = new Set([relay.url, decoy.url].map((url) => new URL(url).host));
  // The public relays answer only a test that asks for them.
  const relays = {
    has: (host: string) =>
      local.has(host) || (options.publicRelays === true && isPublicRelay(host)),
  };
  const hostPort = new URL(host.origin).port;
  const chains = new Set(
    Object.values(NETWORKS.paseo.chains).map(
      (chain) => new URL(chain.rpc).host,
    ),
  );
  const attempted: string[] = [];
  await context.routeWebSocket(/.*/, (socket) => {
    const url = new URL(socket.url());
    const target = url.host;
    const local = url.hostname === "localhost" || url.hostname === "127.0.0.1";
    if (!(local && url.port === hostPort) && !chains.has(target))
      attempted.push(target);
    if (relays.has(target)) socket.connectToServer();
    else socket.close();
  });
  const outside: string[] = [];
  await context.route(/.*/, (route) => {
    const url = route.request().url();
    if (url.startsWith(host.origin) || relays.has(new URL(url).host))
      return route.fallback();
    outside.push(url);
    return route.abort();
  });
  const errors: string[] = [];
  const sockets: Sockets = new Map();
  return {
    context,
    outside,
    errors,
    sockets,
    attempted,
    open: async () => {
      const page = await context.newPage();
      const opened: string[] = [];
      sockets.set(page, opened);
      // The sockets of the page and its frames that a relay answered.
      page.on("websocket", (socket) => {
        const target = new URL(socket.url()).host;
        if (relays.has(target)) opened.push(target);
      });
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(host.origin);
      await ready(page);
      return page;
    },
  };
}

interface Transfer {
  type: string;
  account: string;
  signer?: string;
  sender?: string;
  envelopes?: [number, number] | null;
  count?: number;
  error?: string;
  at: number;
}

interface WorkerWindow {
  __kettleChatWorker?: {
    stage: string;
    reason?: string;
    errors: string[];
    iroh: { relays: string; relayUrl?: string } | null;
    nodes: Record<
      string,
      { state: string; relays?: string; relay?: string; reason?: string }
    >;
    transfers: Transfer[];
    record(): string | null;
  };
}

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });

/**
 * Kettle Chat's iroh setting in the Chat tab: off with null, the public test
 * relays with "public", or one custom relay's URL. It reloads the tab.
 */
async function useIroh(page: Page, relayUrl: string | null): Promise<void> {
  await openTools(page, "chat");
  if ((await page.isChecked("#kettle-iroh")) !== (relayUrl !== null))
    await page.click("#kettle-iroh");
  if (relayUrl === "public")
    await page.selectOption("#kettle-iroh-relays", "public");
  else if (relayUrl !== null) {
    await page.selectOption("#kettle-iroh-relays", "custom");
    await page.fill("#kettle-iroh-relay", relayUrl);
  }
  await Promise.all([
    page.waitForEvent("load"),
    page.click("#kettle-iroh-apply"),
  ]);
  await ready(page);
}

/** Kettle Chat alone, `relayUrl` for its log transfer, acting as `user`. */
async function actAs(
  page: Page,
  user: string,
  relayUrl: string | null,
): Promise<void> {
  await useChatProducts(page, { contact: false, kettle: true });
  await ready(page);
  await useIroh(page, relayUrl);
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
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (frame.url().includes("__truapi-worker.html")) {
        const stage = await frame
          .evaluate(
            () => (window as unknown as WorkerWindow).__kettleChatWorker?.stage,
          )
          .catch(() => undefined);
        if (stage === "running") return frame;
      }
    await page.waitForTimeout(100);
  }
  throw new Error("Kettle Chat's worker did not start");
}

const worker = async (page: Page) =>
  (await workerFrame(page)).evaluate(
    () => (window as unknown as WorkerWindow).__kettleChatWorker!,
  );

const chat = (page: Page) => page.click('#dock [data-destination="chat"]');
const row = (page: Page, title: string) =>
  page.locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`, {
    hasText: title,
  });
const bubble = (page: Page, text: string) =>
  page.locator(".chat-bubble-text", { hasText: new RegExp(`^${text}$`) });

async function allowStatements(page: Page): Promise<void> {
  const prompt = page.locator("dialog.prompt", { hasText: "StatementSubmit" });
  await prompt.getByRole("button", { name: "Always allow" }).click();
  await prompt.waitFor({ state: "detached" });
}

/** Alice invites Bob, he accepts, and each has the other's first message. */
async function connect(alice: Page, bob: Page): Promise<void> {
  await chat(alice);
  await alice.click('[aria-label="New chat"]');
  await alice
    .locator("dialog.prompt", { hasText: "New chat" })
    .locator("button", { hasText: "Bob" })
    .click();
  await alice.fill(".chat-new-request textarea", "Shall we talk?");
  await alice.click('[data-action="start-chat"]');
  await alice
    .locator("dialog.prompt", { hasText: "Send invitation" })
    .getByRole("button", { name: "Send", exact: true })
    .click();
  await chat(bob);
  await bob.click('#chat-tabs [data-chat-tab="inbox"]');
  await bob
    .locator(".notification-row")
    .first()
    .locator('[data-action="notification-accept"]')
    .click();
  await allowStatements(bob);
  await bob.click('#chat-tabs [data-chat-tab="chats"]');
  await row(bob, "Alice").click();
  await alice
    .locator(".chat-system", { hasText: "They accepted your chat request" })
    .waitFor();
  await allowStatements(alice);
  await bubble(bob, "Shall we talk\\?").waitFor();
  await bob.fill(".chat-composer-input", "Yes");
  await bob.press(".chat-composer-input", "Enter");
  await bubble(alice, "Yes").waitFor();
}

async function send(page: Page, text: string): Promise<void> {
  await page.fill(".chat-composer-input", text);
  await page.press(".chat-composer-input", "Enter");
}

describe("Kettle Chat's log recovery over iroh", () => {
  test.skipIf(!runs)(
    "a relay that is not reached leaves that side on the Statement Store alone, saying why, and the chat works; a side with iroh off opens no relay connection",
    async () => {
      const { context, outside, errors, sockets, attempted, open } =
        await offline();
      const before = await relayConnections();
      try {
        const alice = await open();
        await actAs(alice, "alice", null);
        const bob = await open();
        // Nothing listens on port 9.
        await actAs(bob, "bob", "http://127.0.0.1:9/");
        expect((await worker(bob)).iroh).toEqual({
          relays: "custom",
          relayUrl: "http://127.0.0.1:9/",
        });
        await connect(alice, bob);
        await send(alice, "Still here");
        await bubble(bob, "Still here").waitFor();
        const [aliceWorker, bobWorker] = [
          await worker(alice),
          await worker(bob),
        ];
        expect([
          aliceWorker.iroh,
          Object.values(aliceWorker.nodes).map((status) => status.state),
          Object.values(bobWorker.nodes),
          bobWorker.errors.filter((line) => line.startsWith("iroh:")).length,
          await relayConnections(),
          errors,
        ]).toEqual([
          null,
          ["off"],
          [
            {
              state: "unavailable",
              relays: "custom",
              relay: "http://127.0.0.1:9/",
              reason: "the endpoint reached no relay in 10000 ms",
            },
          ],
          1,
          before,
          [],
        ]);
        // The only requests outside the host: iroh's probes of the relay it was given.
        expect(
          outside.every((url) => url.startsWith("http://127.0.0.1:9/")),
        ).toBe(true);
        // No tab asked for a socket to anywhere but Bob's relay, and Alice,
        // off, reached none.
        expect([
          attempted.filter((target) => target !== "127.0.0.1:9"),
          sockets.get(alice),
        ]).toEqual([[], []]);

        // Alice turns iroh on mid-chat, then off again: each reloads her
        // tab, and the chat, its history and Kettle's state stay.
        for (const relayUrl of [relay.url, null]) {
          await useIroh(alice, relayUrl);
          await closeTools(alice);
          // The node starts once the worker runs and its account may post.
          let status = Object.values((await worker(alice)).nodes);
          for (let wait = 0; status.length === 0 && wait < 600; wait += 1) {
            await alice.waitForTimeout(100);
            status = Object.values((await worker(alice)).nodes);
          }
          expect(status.map((each) => [each.state, each.relay])).toEqual([
            relayUrl === null ? ["off", undefined] : ["on", relay.url],
          ]);
          await chat(alice);
          await row(alice, "Bob").click();
          await bubble(alice, "Still here").waitFor();
          expect(await bubble(alice, "Yes").count()).toBe(1);
          expect(await bubble(alice, "Still here").count()).toBe(1);
          const text = `Toggled ${relayUrl === null ? "off" : "on"}`;
          await send(bob, text);
          await bubble(alice, text).waitFor();
          await send(alice, `${text}, reply`);
          await bubble(bob, `${text}, reply`).waitFor();
        }
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test.skipIf(!runs)(
    "Bob away while Alice sends more than her outbox holds, and her new outboxes do not reach the store: on his return from saved state he fetches the rest of her log over iroh through the relay, gets every message once and in order, and they go on talking",
    () => awayAndBack({ alice: "relay", bob: "relay" }),
    TEST_MS,
  );

  test.skipIf(!runs)(
    "control, iroh off: the same gap leaves Bob with only what the stored outbox holds; once Alice's outboxes reach the store again, the Statement Store alone brings the rest",
    () => awayAndBack({ alice: null, bob: null }),
    TEST_MS,
  );

  test.skipIf(!runs)(
    "the public test relays by default: the choice holds across reloads in this tab only, a refused custom relay changes nothing and is not repeated, and with every public relay unreachable the node says so and the chat runs on the Statement Store alone",
    async () => {
      const { context, outside, errors, sockets, attempted, open } =
        await offline();
      try {
        const alice = await open();
        await useChatProducts(alice, { contact: false, kettle: true });
        await ready(alice);
        await openTools(alice, "chat");
        // On, with nothing else chosen: the public test relays.
        expect([
          await alice.isChecked("#kettle-iroh"),
          await alice.inputValue("#kettle-iroh-relays"),
          await alice.isDisabled("#kettle-iroh-relays"),
          await alice.isHidden("#kettle-iroh-relay"),
        ]).toEqual([false, "public", true, true]);
        await alice.click("#kettle-iroh");
        await Promise.all([
          alice.waitForEvent("load"),
          alice.click("#kettle-iroh-apply"),
        ]);
        await ready(alice);
        await openTools(alice, "chat");
        const secret = "synthetic-secret-5";
        await alice.selectOption("#kettle-iroh-relays", "custom");
        await alice.fill(
          "#kettle-iroh-relay",
          `https://user:${secret}@relay.example`,
        );
        await alice.click("#kettle-iroh-apply");
        const refusal = await alice.locator("#kettle-iroh-status").innerText();
        expect(refusal.startsWith("The relay must be")).toBe(true);
        expect(refusal.includes(secret)).toBe(false);
        await alice.reload();
        await ready(alice);
        await openTools(alice, "chat");
        expect([
          await alice.isChecked("#kettle-iroh"),
          (await alice.inputValue("#kettle-iroh-relays")) === "public",
          (await alice.locator("#kettle-iroh-status").innerText()).includes(
            "public test relays",
          ),
        ]).toEqual([true, true, true]);
        await closeTools(alice);
        // Another tab has its own setting: off.
        const other = await open();
        await openTools(other, "chat");
        expect(await other.isChecked("#kettle-iroh")).toBe(false);
        await other.close();

        await useInvitationDemo(alice, "alice");
        await ready(alice);
        await closeTools(alice);
        const bob = await open();
        await actAs(bob, "bob", null);
        await connect(alice, bob);
        await send(alice, "Over the store");
        await bubble(bob, "Over the store").waitFor();
        const nodes = Object.values((await worker(alice)).nodes);
        // The public relays are not named: the report says no "(undefined)".
        expect(
          (await worker(alice)).errors.filter((line) =>
            line.startsWith("iroh:"),
          ),
        ).toEqual([
          "iroh: the endpoint reached no relay in 10000 ms; this chat runs on the Statement Store alone",
        ]);
        console.log(
          JSON.stringify({
            blockedPublicSockets: [...new Set(attempted)],
            blockedPublicRequests: [
              ...new Set(outside.map((url) => new URL(url).host)),
            ],
          }),
        );
        // Alice's endpoint did try the public relays, and only them: their
        // HTTPS probes, refused here, come before any socket.
        expect(outside.length).toBeGreaterThan(0);
        expect([
          (await worker(alice)).iroh,
          nodes,
          sockets.get(bob),
          attempted.every(isPublicRelay),
          outside.every((url) => isPublicRelay(new URL(url).host)),
          errors,
        ]).toEqual([
          { relays: "public" },
          [
            {
              state: "unavailable",
              relays: "public",
              reason: "the endpoint reached no relay in 10000 ms",
            },
          ],
          [],
          true,
          true,
          [],
        ]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );

  test.skipIf(!runs || !PUBLIC)(
    "PUBLIC SMOKE (KETTLE_IROH_PUBLIC=1): two tabs on n0's public test relays recover Bob's missed messages by a log fetch",
    () => awayAndBack({ alice: "public", bob: "public" }, true),
    TEST_MS,
  );

  test.skipIf(!runs)(
    "a peer whose address names another relay gets no connection: Bob, on the configured relay, never reaches Alice's relay, and only the Statement Store brings her messages",
    () => awayAndBack({ alice: "decoy", bob: "relay" }),
    TEST_MS,
  );
});

const STATEMENTS = "truapi-web-signing-host:local-statements:";

/**
 * Keep `page`'s new statements out of the shared simulated store, or let
 * them in again: its node still runs and believes it posted. Only this
 * test's tabs are changed.
 */
async function holdStatements(page: Page, held: boolean): Promise<void> {
  await page.evaluate(
    ([prefix, held]) => {
      // Both: a held statement neither lands nor replaces the one stored.
      const keep = window as unknown as {
        __storage?: Pick<Storage, "setItem" | "removeItem">;
      };
      keep.__storage ??= {
        setItem: Storage.prototype.setItem,
        removeItem: Storage.prototype.removeItem,
      };
      const { setItem, removeItem } = keep.__storage;
      const skip = (key: string) => held && key.startsWith(prefix as string);
      Storage.prototype.setItem = function (this: Storage, key, value) {
        if (!skip(key)) setItem.call(this, key, value);
      };
      Storage.prototype.removeItem = function (this: Storage, key) {
        if (!skip(key)) removeItem.call(this, key);
      };
    },
    [STATEMENTS, held] as const,
  );
}

/** Which relay each side uses: the configured one, the decoy, or none. */
interface Sides {
  alice: Side;
  bob: Side;
}
type Side = "relay" | "decoy" | "public" | null;

async function awayAndBack(sides: Sides, publicRelays = false): Promise<void> {
  const { context, outside, errors, sockets, attempted, open } = await offline({
    publicRelays,
  });
  const before = await relayConnections();
  const decoyBefore = await relayConnections(decoy);
  const urlOf = (side: Side) =>
    side === null
      ? null
      : side === "public"
        ? "public"
        : side === "relay"
          ? relay.url
          : decoy.url;
  /** Which side a relay host is of: a public relay counts as "public". */
  const sideOf = (host: string): Side | string =>
    host === new URL(relay.url).host
      ? "relay"
      : host === new URL(decoy.url).host
        ? "decoy"
        : isPublicRelay(host)
          ? "public"
          : host;
  const onRelay = (side: Side) => (side === "relay" ? 1 : 0);
  const onDecoy = (side: Side) => (side === "decoy" ? 1 : 0);
  // Bob recovers by a fetch only from a peer on his own relay.
  const fetches = sides.bob !== null && sides.alice === sides.bob;
  try {
    const alice = await open();
    await actAs(alice, "alice", urlOf(sides.alice));
    const bob = await open();
    await actAs(bob, "bob", urlOf(sides.bob));
    await connect(alice, bob);
    const states = [
      Object.values((await worker(alice)).nodes),
      Object.values((await worker(bob)).nodes),
    ].map((list) =>
      list.map((status) => [
        status.state,
        status.relays ?? null,
        status.relay === undefined ? null : sideOf(new URL(status.relay).host),
      ]),
    );
    expect(states).toEqual(
      [sides.alice, sides.bob].map((side) => [
        side === null
          ? ["off", null, null]
          : ["on", side === "public" ? "public" : "custom", side],
      ]),
    );
    expect((await relayConnections()).open).toBe(
      before.open + onRelay(sides.alice) + onRelay(sides.bob),
    );

    // Bob's tab leaves the host: his worker and endpoint go, his saved
    // Kettle state and history stay in this browser.
    await bob.goto("about:blank");
    const texts = Array.from({ length: MISSED }, (_, i) => `m${i + 1}`);
    for (const text of texts) await send(alice, text);
    // Hold only once Alice's node published every message: the stored
    // outbox then names her whole log.
    const until = Date.now() + STEP_MS;
    while (Date.now() < until) {
      const record = JSON.parse(
        (await (
          await workerFrame(alice)
        ).evaluate(() =>
          (window as unknown as WorkerWindow).__kettleChatWorker!.record(),
        )) ?? "null",
      ) as { conversations: { messages: { text: string; state: string }[] }[] };
      const states = record.conversations[0]!.messages.filter((message) =>
        /^m\d+$/.test(message.text),
      ).map((message) => message.state);
      if (states.length === MISSED && states.every((state) => state === "sent"))
        break;
      await alice.waitForTimeout(200);
    }
    await alice.waitForTimeout(1_000);
    await holdStatements(alice, true);

    const returned = Date.now();
    await bob.goto(host.origin);
    await ready(bob);
    await workerFrame(bob);
    await chat(bob);
    await row(bob, "Alice").click();
    const missed = async () =>
      (await bob.locator(".chat-bubble-text").allInnerTexts()).filter((text) =>
        /^m\d+$/.test(text),
      );
    // A miss fails below, with what did arrive.
    if (fetches)
      await bubble(bob, `m${MISSED}`)
        .waitFor()
        .catch(() => undefined);
    else await bob.waitForTimeout(20_000);
    const recoveredMs = Date.now() - returned;
    const got = await missed();
    const transfers = (await worker(bob)).transfers;
    const window = transfers.find(
      (t) => t.type === "outboxReceived" && t.envelopes != null,
    )!.envelopes!;
    const fetched = transfers.filter((t) => t.type === "fetchFinished");
    console.log(
      JSON.stringify({
        sides,
        recoveredMs,
        firstWindow: window,
        received: got.length,
        fetched: fetched.map((t) => t.count ?? t.error),
        fetchMs: fetched.map((t) => t.at - returned),
      }),
    );
    // The outbox in the store held fewer than the messages Bob missed.
    expect(window[1] - window[0] + 1).toBeLessThan(MISSED);
    if (fetches) {
      expect(fetched.length).toBeGreaterThan(0);
      expect(fetched.every((t) => t.error === undefined)).toBe(true);
      expect(got).toEqual(texts);
    } else {
      // With both on, but on different relays, Bob tries to fetch from Alice
      // and his endpoint refuses it before any packet leaves.
      if (sides.alice !== null && sides.bob !== null) {
        expect(fetched.length).toBeGreaterThan(0);
        expect(
          fetched.every((t) => /rejected locally/.test(t.error ?? "")),
        ).toBe(true);
      } else expect(fetched).toEqual([]);
      expect(got).toEqual(texts.slice(0, got.length));
      expect(got.length).toBeLessThan(MISSED);
    }
    // No tab asked for a socket to anywhere but the relays the tabs were
    // given, and each tab reached its own relay only.
    const given = new Set<Side | string>([sides.alice, sides.bob]);
    expect(attempted.filter((target) => !given.has(sideOf(target)))).toEqual(
      [],
    );
    expect([
      [...new Set(sockets.get(alice)!.map(sideOf))],
      [...new Set(sockets.get(bob)!.map(sideOf))],
    ]).toEqual(
      [sides.alice, sides.bob].map((side) => (side === null ? [] : [side])),
    );
    // Open connections, not accepts: an endpoint may reconnect to its relay.
    expect((await relayConnections(decoy)).open - decoyBefore.open).toBe(
      onDecoy(sides.alice) + onDecoy(sides.bob),
    );

    await holdStatements(alice, false);
    if (!fetches) {
      await bubble(bob, `m${MISSED}`).waitFor();
      expect(await missed()).toEqual(texts);
    }
    await send(alice, "After your return");
    await bubble(bob, "After your return").waitFor();
    await send(bob, "Got all of them");
    await bubble(alice, "Got all of them").waitFor();
    expect(await bubble(bob, "After your return").count()).toBe(1);
    expect([outside, errors]).toEqual([[], []]);
  } finally {
    await context.close();
  }
  // Each tab's endpoint went with its page: no relay connection stays.
  for (let wait = 0; wait < 50; wait += 1) {
    if ((await relayConnections()).open === before.open) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  expect((await relayConnections()).open).toBe(before.open);
}
