import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, Frame, Page } from "playwright-core";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { decodeStatement, statementHash } from "../src/platform/statement.js";
import { blake2b256 } from "../src/platform/crypto.js";
import { PEOPLE } from "./support/guarded-posting-harness.js";
import {
  closeTools,
  probeChatWorkers,
  reveal,
  startHost,
  testBrowser,
  useChatProducts,
  useInvitationDemo,
  type RunningHost,
} from "./support/fixtures.js";

const BASE = `0x${"11".repeat(32)}`;
const CLAIM = `0x${"22".repeat(32)}`;
const CHILD = `0x${"33".repeat(32)}`;
const ORPHAN = `0x${"44".repeat(32)}`;
const GRANT = "0x0200000000d00700";
const NOW = Date.UTC(2026, 10, 10, 12);
const METHODS = [
  "chainHead_v1_follow",
  "chainHead_v1_storage",
  "chain_getHeader",
  "chain_getBlockHash",
  "chain_getFinalizedHead",
  "state_getStorage",
  "statement_submit",
];
const TIMEOUT = 240_000;

interface Actor {
  mode: "pending" | "final" | "orphaned";
  refuse: number;
  dropDelivery: boolean;
  claims: { account: string; product: string; index?: string }[];
  posts: string[];
  leases: {
    account: string;
    product: string;
    until: number;
    provisional?: true;
  }[];
  usable: { usableAt: string; claimFinal: boolean; account: string }[];
  rpc: string[];
  logs: string[];
  backupWaiting: number;
  backupFinal: number;
}

class ControlledChain {
  private next = 0;
  private readonly actors = new Map<string, Actor>();
  snapshot() {
    return Object.fromEntries(this.actors);
  }
  actor(name: string): Actor {
    let actor = this.actors.get(name);
    if (actor === undefined) {
      actor = {
        mode: "pending",
        refuse: 0,
        dropDelivery: false,
        claims: [],
        posts: [],
        leases: [],
        usable: [],
        rpc: [],
        logs: [],
        backupWaiting: 0,
        backupFinal: 0,
      };
      this.actors.set(name, actor);
    }
    return actor;
  }
  answer = async ({
    actor: name,
    op,
    value,
  }: {
    actor: string;
    op: string;
    value: unknown;
  }): Promise<unknown> => {
    const actor = this.actor(name);
    switch (op) {
      case "claim":
        actor.claims.push(value as Actor["claims"][number]);
        return { seq: 0, blockHash: CLAIM };
      case "submit":
        actor.posts.push(value as string);
        if (actor.refuse > 0) {
          actor.refuse -= 1;
          return true;
        }
        return false;
      case "lease":
        actor.leases.push(value as Actor["leases"][number]);
        return null;
      case "deliver":
        return !actor.dropDelivery;
      case "usable":
        actor.usable.push(value as Actor["usable"][number]);
        return null;
      case "log":
        actor.logs.push(String(value));
        return null;
      case "backup-wait":
        actor.backupWaiting += 1;
        return null;
      case "backup-final":
        actor.backupFinal += 1;
        return null;
      case "rpc":
        return this.rpc(
          actor,
          value as { id: unknown; method: string; params: unknown[] },
        );
      default:
        throw new Error(`Unexpected controlled operation ${op}`);
    }
  };
  private rpc(
    actor: Actor,
    request: { id: unknown; method: string; params: unknown[] },
  ): unknown[] {
    const { id, method, params } = request;
    actor.rpc.push(method);
    const result = (value: unknown) => ({ jsonrpc: "2.0", id, result: value });
    const event = (subscription: unknown, value: unknown) => ({
      jsonrpc: "2.0",
      method: "chainHead_v1_followEvent",
      params: { subscription, result: value },
    });
    switch (method) {
      case "system_version":
        return [result("1.25.1-4a0cc2e7449")];
      case "rpc_methods":
        return [result({ methods: METHODS })];
      case "chain_getBlockHash":
        return [
          result(
            params[0] === 0
              ? PEOPLE
              : actor.mode === "orphaned"
                ? ORPHAN
                : CLAIM,
          ),
        ];
      case "chain_getFinalizedHead":
        return [result(actor.mode === "pending" ? BASE : CHILD)];
      case "chain_getHeader":
        return [
          result({
            number:
              params[0] === BASE
                ? "0xa"
                : params[0] === CLAIM || params[0] === ORPHAN
                  ? "0xb"
                  : "0xc",
          }),
        ];
      case "state_getStorage":
        return [result(actor.mode === "final" ? GRANT : null)];
      case "chainHead_v1_follow": {
        const subscription = `controlled-${++this.next}`;
        return [
          result(subscription),
          event(subscription, {
            event: "initialized",
            finalizedBlockHashes: [actor.mode === "final" ? CHILD : BASE],
          }),
          event(subscription, {
            event: "newBlock",
            blockHash: CLAIM,
            parentBlockHash: BASE,
          }),
          event(subscription, {
            event: "newBlock",
            blockHash: CHILD,
            parentBlockHash: CLAIM,
          }),
          event(subscription, {
            event: "bestBlockChanged",
            bestBlockHash: CHILD,
          }),
        ];
      }
      case "chainHead_v1_storage": {
        const operationId = `storage-${++this.next}`;
        const items = params[2] as { key: string }[];
        return [
          result({ result: "started", operationId }),
          event(params[0], {
            event: "operationStorageItems",
            operationId,
            items: items.map(({ key }) => ({
              key,
              value:
                params[1] === CHILD || actor.mode === "final" ? GRANT : null,
            })),
          }),
          event(params[0], { event: "operationStorageDone", operationId }),
        ];
      }
      case "chainHead_v1_unfollow":
        return [result(true)];
      default:
        throw new Error(`No real RPC fallback: ${method}`);
    }
  }
}

let browser: Browser;
let host: RunningHost;
beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, 60_000);
afterAll(async () => {
  await host?.close();
});

async function until(
  check: () => boolean,
  what: string,
  timeout = 60_000,
): Promise<void> {
  const end = Date.now() + timeout;
  while (!check()) {
    if (Date.now() > end) throw new Error(`Waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

async function offline() {
  const context = await browser.newContext({
    serviceWorkers: "allow",
    viewport: { width: 1280, height: 860 },
  });
  context.setDefaultTimeout(60_000);
  const chain = new ControlledChain();
  const outside: string[] = [];
  const errors: string[] = [];
  let clockInstalled = false;
  await probeChatWorkers(context);
  await context.exposeFunction("__postingControl", chain.answer);
  await context.routeWebSocket(/.*/, (socket) => socket.close());
  await context.route(/.*/, async (route) => {
    const url = new URL(route.request().url());
    if (url.origin !== host.origin) {
      outside.push(url.href);
      return route.abort();
    }
    if (url.pathname === "/") {
      const response = await route.fetch();
      const html = await response.text();
      const entry = '<script type="module" src="/src/main.ts"></script>';
      expect(html).toContain(entry);
      return route.fulfill({
        response,
        body: html.replace(
          entry,
          '<script type="module">const {installGuardedPosting}=await import("/browser/support/guarded-posting-harness.ts");installGuardedPosting();await import("/src/main.ts");</script>',
        ),
      });
    }
    return route.fallback();
  });
  return {
    context,
    chain,
    outside,
    errors,
    open: async (
      actor: string,
      product: "contact" | "kettle" | "nova" = "nova",
    ) => {
      const page = await context.newPage();
      page.on("pageerror", (error) => errors.push(error.message));
      if (!clockInstalled) {
        await page.clock.install({ time: NOW });
        clockInstalled = true;
      }
      await page.goto(`${host.origin}/?actor=${actor}`);
      await ready(page);
      await useChatProducts(page, {
        contact: product === "contact",
        kettle: product === "kettle",
        nova: product === "nova",
      });
      await ready(page);
      await useInvitationDemo(page, actor);
      await ready(page);
      await page
        .locator("#session")
        .filter({ hasNotText: "Signed out" })
        .waitFor({ state: "attached" });
      await closeTools(page);
      return page;
    },
  };
}

async function evidence(
  run: Awaited<ReturnType<typeof offline>>,
  name: string,
) {
  const pages = [];
  for (const page of run.context.pages()) {
    pages.push({
      url: page.url(),
      text: await page
        .locator("body")
        .innerText()
        .catch(() => "closed"),
      log: await page
        .locator("#log")
        .textContent()
        .catch(() => ""),
      frames: await Promise.all(
        page.frames().map(async (frame) => ({
          url: frame.url(),
          probes: await frame
            .evaluate(() =>
              Object.fromEntries(
                Object.entries(window)
                  .filter(([key]) =>
                    [
                      "__novaChatWorker",
                      "__kettleChatWorker",
                      "__contactChatWorker",
                    ].includes(key),
                  )
                  .map(([key, value]) => [key, JSON.stringify(value)]),
              ),
            )
            .catch(() => null),
        })),
      ),
    });
  }
  await Bun.write(
    `${process.env.ZDX_ARTIFACT_DIR}/${name}.json`,
    JSON.stringify(
      {
        chain: run.chain.snapshot(),
        errors: run.errors,
        outside: run.outside,
        pages,
      },
      null,
      2,
    ),
  );
}

const ready = (page: Page) =>
  page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
const item = (page: Page) =>
  page.locator('.notification-row[data-notification^="nova-chat:"]');
const bubble = (page: Page, text: string) =>
  page.locator(".chat-bubble-text").filter({ hasText: text });

async function list(page: Page, tab: "chats" | "inbox" = "chats") {
  await page.click('#dock [data-destination="chat"]');
  const back = page.locator('[aria-label="Back to chats"]');
  if (await back.isVisible()) await back.click();
  await page.click(`#chat-tabs [data-chat-tab="${tab}"]`);
}

async function request(page: Page, welcome: string) {
  await list(page);
  await page.click('[aria-label="New chat"]');
  await page
    .locator("dialog.prompt", { hasText: "New chat" })
    .locator("button", { hasText: "Bob" })
    .click();
  await page.fill(".chat-new-request textarea", welcome);
  await page.click('[data-action="start-chat"]', { timeout: 15_000 });
  await page
    .locator("dialog.prompt", { hasText: "Send invitation" })
    .getByRole("button", { name: "Send", exact: true })
    .click();
}

async function accept(page: Page, nova = true) {
  await list(page, "inbox");
  await page.locator('[data-action="notification-accept"]').first().click();
  if (nova)
    await page
      .locator("dialog.prompt", { hasText: "Accept chat request" })
      .getByRole("button", { name: "Accept", exact: true })
      .click();
}

async function say(page: Page, product: string, peer: string, text: string) {
  await list(page);
  await page
    .locator(`[data-conversation^="chat-v2:${product}.paseo/"]`, {
      hasText: peer,
    })
    .click();
  await page.locator(".chat-composer-input").fill(text);
  await page.press(".chat-composer-input", "Enter");
  await bubble(page, text).waitFor();
  await page
    .locator(
      '.chat-message-out[data-status="sent"], .chat-message-out[data-status="delivered"]',
      { hasText: text },
    )
    .waitFor();
}

function allowStatements(page: Page): () => void {
  const timer = setInterval(() => {
    void page
      .locator("dialog.prompt", { hasText: "StatementSubmit" })
      .getByRole("button", { name: "Always allow" })
      .dispatchEvent("click", { timeout: 100 })
      .catch(() => {});
  }, 100);
  return () => clearInterval(timer);
}

function signatures(posts: string[]) {
  return posts.map((encoded) => ({
    hash: bytesToHex(statementHash(hexToBytes(encoded))),
    statement: decodeStatement(hexToBytes(encoded)),
  }));
}

interface NovaProbeWindow {
  __novaChatWorker?: { record(): string | null };
}

async function novaFrame(page: Page): Promise<Frame> {
  const end = Date.now() + 60_000;
  while (Date.now() < end) {
    for (const frame of page.frames())
      if (
        await frame
          .evaluate(() => "__novaChatWorker" in window)
          .catch(() => false)
      )
        return frame;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error("The synthetic Nova worker is not running.");
}

async function novaPending(page: Page) {
  const frame = await novaFrame(page);
  return frame.evaluate(
    () =>
      Object.values(
        JSON.parse(
          (window as unknown as NovaProbeWindow).__novaChatWorker!.record()!,
        ) as Record<
          string,
          {
            pending: { id: string; encoded: string }[];
            history: { id: string; status: string }[];
          }
        >,
      )[0]!,
  );
}

describe("guarded best-ready chat in isolated browsers", () => {
  test(
    "Nova accepts at best, retries identical noAllowance bytes, exchanges first messages before finality and resumes confirmation after reload",
    async () => {
      const run = await offline();
      const alice = await run.open("alice");
      const bob = await run.open("bob");
      const stopA = allowStatements(alice),
        stopB = allowStatements(bob);
      try {
        await request(alice, "best request");
        await until(
          () => run.chain.actor("alice").posts.length > 0,
          "request at best",
        );
        run.chain.actor("bob").refuse = 1;
        await accept(bob);
        await until(
          () => run.chain.actor("bob").posts.length > 0,
          "first refused acceptance",
        );
        await bob.clock.fastForward(2_100);
        await until(
          () => run.chain.actor("bob").posts.length >= 2,
          "same-byte retry",
        );
        expect(run.chain.actor("bob").posts[0]).toBe(
          run.chain.actor("bob").posts[1],
        );
        await say(alice, "nova-chat", "Bob", "first inviter at best");
        await say(bob, "nova-chat", "Alice", "first recipient at best");
        await bubble(bob, "first inviter at best").waitFor();
        await list(alice);
        await alice
          .locator('[data-conversation^="chat-v2:nova-chat.paseo/"]', {
            hasText: "Bob",
          })
          .click();
        await bubble(alice, "first recipient at best").waitFor();
        for (const name of ["alice", "bob"]) {
          const actor = run.chain.actor(name);
          const currentTime = await (name === "alice" ? alice : bob).evaluate(
            () => Date.now(),
          );
          expect(actor.mode).toBe("pending");
          expect(
            actor.usable.some(
              (seen) => seen.usableAt === "best" && !seen.claimFinal,
            ),
          ).toBe(true);
          expect(
            actor.leases.some(
              (lease) =>
                lease.provisional && lease.until <= currentTime + 180_000,
            ),
          ).toBe(true);
          expect(actor.rpc).toContain("system_version");
          expect(actor.rpc).toContain("chainHead_v1_storage");
        }
        await list(bob, "inbox");
        expect(await item(bob).innerText()).toContain("Checking its claim");
        const notificationClaims = () =>
          run.chain
            .actor("bob")
            .claims.filter((claim) => claim.product === "notification").length;
        const acceptanceAccount = decodeStatement(
          hexToBytes(run.chain.actor("bob").posts[0]!),
        ).signer;
        const acceptancePosts = () =>
          signatures(run.chain.actor("bob").posts).filter(
            (post) =>
              bytesToHex(post.statement.signer) ===
              bytesToHex(acceptanceAccount),
          ).length;
        const claims = notificationClaims();
        const posts = acceptancePosts();
        await bob.reload();
        await ready(bob);
        await closeTools(bob);
        await list(bob, "inbox");
        expect(await item(bob).getAttribute("data-status")).toBe("pending");
        expect(notificationClaims()).toBe(claims);
        run.chain.actor("bob").mode = "final";
        await bob.clock.fastForward(3_100);
        await until(
          () =>
            run.chain
              .actor("bob")
              .usable.some(
                (seen) => seen.usableAt === "finalized" && seen.claimFinal,
              ),
          "saved inclusion finalized",
        );
        await item(bob).filter({ hasText: "Accepted" }).waitFor();
        expect(acceptancePosts()).toBe(posts);
        expect(notificationClaims()).toBe(claims);
        expect(run.errors).toEqual([]);
      } finally {
        stopA();
        stopB();
        await evidence(run, "nova-best");
        await run.context.close();
      }
    },
    TIMEOUT,
  );

  test(
    "Nova keeps an unacknowledged first message pending, renews its provisional account and delivers once after the recipient reloads",
    async () => {
      const run = await offline();
      const alice = await run.open("alice"),
        bob = await run.open("bob");
      const stopA = allowStatements(alice),
        stopB = allowStatements(bob);
      try {
        await request(alice, "pending frame");
        await accept(bob);
        await until(
          () => run.chain.actor("bob").posts.length === 1,
          "best acceptance",
        );
        run.chain.actor("bob").dropDelivery = true;
        await say(alice, "nova-chat", "Bob", "unacknowledged first");
        await until(
          () => run.chain.actor("alice").leases.length === 1,
          "first provisional lease",
        );
        const pending = await novaPending(alice);
        expect(pending.pending).toHaveLength(1);
        expect(pending.history[0]!.status).not.toBe("acked");
        const posts = run.chain.actor("alice").posts.length;
        run.chain.actor("alice").mode = "final";
        run.chain.actor("bob").mode = "final";
        await alice.clock.fastForward(181_100);
        await until(
          () => run.chain.actor("alice").posts.length > posts,
          "pending frame renewed",
        );
        expect(
          run.chain.actor("alice").leases.at(-1)?.provisional,
        ).toBeUndefined();
        expect((await novaPending(alice)).pending).toEqual(pending.pending);
        expect(
          new Set(run.chain.actor("alice").claims.map((claim) => claim.account))
            .size,
        ).toBe(2);
        const sent = signatures(run.chain.actor("alice").posts).filter(
          (post) =>
            bytesToHex(post.statement.signer) ===
            run.chain.actor("alice").leases[0]!.account,
        );
        expect(sent.at(-1)!.statement.expiry > sent[0]!.statement.expiry).toBe(
          true,
        );
        run.chain.actor("bob").dropDelivery = false;
        await bob.reload();
        await ready(bob);
        await closeTools(bob);
        await say(bob, "nova-chat", "Alice", "recipient after replay");
        await (
          await novaFrame(alice)
        ).waitForFunction(() =>
          Object.values(
            JSON.parse(
              (
                window as unknown as NovaProbeWindow
              ).__novaChatWorker!.record()!,
            ) as Record<string, { pending: unknown[] }>,
          ).every((record) => record.pending.length === 0),
        );
        await bubble(bob, "unacknowledged first").waitFor();
        expect(await bubble(bob, "unacknowledged first").count()).toBe(1);
        expect(run.errors).toEqual([]);
      } finally {
        stopA();
        stopB();
        await evidence(run, "nova-renewal");
        await run.context.close();
      }
    },
    TIMEOUT,
  );

  test(
    "Nova exposes an orphaned acceptance and explicit retry preserves its logical kind 20 payload without another claim",
    async () => {
      const run = await offline();
      const alice = await run.open("alice"),
        bob = await run.open("bob");
      try {
        await request(alice, "recover acceptance");
        await accept(bob);
        await until(
          () => run.chain.actor("bob").posts.length === 1,
          "best acceptance",
        );
        const original = signatures(run.chain.actor("bob").posts)[0]!;
        run.chain.actor("bob").mode = "orphaned";
        await bob.clock.fastForward(3_100);
        await new Promise((resolve) => setTimeout(resolve, 300));
        await bob.clock.fastForward(181_000);
        await item(bob).filter({ hasText: "Select Accept to retry" }).waitFor();
        const claims = run.chain.actor("bob").claims.length;
        await bob.reload();
        await ready(bob);
        await closeTools(bob);
        await list(bob, "inbox");
        expect(await item(bob).innerText()).toContain("Select Accept");
        run.chain.actor("bob").mode = "final";
        await accept(bob);
        await item(bob).filter({ hasText: "Accepted" }).waitFor();
        const replacement = signatures(run.chain.actor("bob").posts).at(-1)!;
        expect(replacement.hash).not.toBe(original.hash);
        expect(replacement.statement).toMatchObject({
          signer: original.statement.signer,
          data: original.statement.data,
          topics: original.statement.topics,
          channel: original.statement.channel,
          expiry: original.statement.expiry + 1n,
        });
        expect(run.chain.actor("bob").claims.length).toBe(claims);
        expect(run.errors).toEqual([]);
      } finally {
        await evidence(run, "nova-retry");
        await run.context.close();
      }
    },
    TIMEOUT,
  );

  for (const product of ["contact", "kettle"] as const) {
    test(
      `${product} exchanges first messages on provisional accounts and renews after the real lease bound`,
      async () => {
        const run = await offline();
        const alice = await run.open("alice", product),
          bob = await run.open("bob", product);
        const stopA = allowStatements(alice),
          stopB = allowStatements(bob);
        const id = `${product}-chat`;
        try {
          await request(alice, "provisional setup");
          await accept(bob, false);
          await say(alice, id, "Bob", "first inviter provisional");
          await say(bob, id, "Alice", "first recipient provisional");
          await bubble(bob, "first inviter provisional").waitFor();
          for (const name of ["alice", "bob"]) {
            const actor = run.chain.actor(name);
            expect(
              actor.usable.some(
                (seen) => seen.usableAt === "best" && !seen.claimFinal,
              ),
            ).toBe(true);
            expect(actor.leases.some((lease) => lease.provisional)).toBe(true);
            expect(
              new Set(actor.claims.map((claim) => claim.account)).size,
            ).toBe(1);
          }
          const beforeA = signatures(run.chain.actor("alice").posts);
          const beforeB = signatures(run.chain.actor("bob").posts);
          run.chain.actor("alice").mode = "final";
          run.chain.actor("bob").mode = "final";
          await alice.clock.fastForward(181_100);
          await until(
            () =>
              run.chain
                .actor("alice")
                .leases.some((lease) => !lease.provisional) &&
              run.chain.actor("bob").leases.some((lease) => !lease.provisional),
            "both leases proved final",
          );
          for (const [name, before] of [
            ["alice", beforeA],
            ["bob", beforeB],
          ] as const) {
            const actor = run.chain.actor(name);
            await until(
              () => actor.posts.length > before.length,
              "pending product republished",
            );
            expect(actor.claims.length).toBe(1);
            const after = signatures(actor.posts);
            expect(
              after.some((post) =>
                before.some(
                  (old) =>
                    bytesToHex(old.statement.data) ===
                      bytesToHex(post.statement.data) &&
                    post.statement.expiry > old.statement.expiry &&
                    post.hash !== old.hash,
                ),
              ),
            ).toBe(true);
          }
          await bob.reload();
          await ready(bob);
          await closeTools(bob);
          await say(bob, id, "Alice", "after reload");
          expect(run.errors).toEqual([]);
        } finally {
          stopA();
          stopB();
          await evidence(run, `${product}-renewal`);
          await run.context.close();
        }
      },
      TIMEOUT,
    );
  }

  test(
    "Contact Chat shows an unconfirmed window as failed and its explicit Retry reuses the same account and logical message",
    async () => {
      const run = await offline();
      const alice = await run.open("alice", "contact"),
        bob = await run.open("bob", "contact");
      const stopA = allowStatements(alice),
        stopB = allowStatements(bob);
      try {
        await request(alice, "window retry");
        await accept(bob, false);
        await say(alice, "contact-chat", "Bob", "window awaiting confirmation");
        await until(
          () => run.chain.actor("alice").posts.length === 1,
          "provisional window",
        );
        const original = signatures(run.chain.actor("alice").posts)[0]!;
        run.chain.actor("alice").mode = "orphaned";
        await alice.clock.fastForward(181_100);
        await alice
          .getByRole("button", { name: "Retry", exact: true })
          .waitFor();
        expect(run.chain.actor("alice").posts.length).toBe(1);
        await alice.reload();
        await ready(alice);
        await closeTools(alice);
        await list(alice);
        await alice
          .locator('[data-conversation^="chat-v2:contact-chat.paseo/"]', {
            hasText: "Bob",
          })
          .click();
        await alice
          .getByRole("button", { name: "Retry", exact: true })
          .waitFor();
        run.chain.actor("alice").mode = "final";
        await alice.getByRole("button", { name: "Retry", exact: true }).click();
        await until(
          () => run.chain.actor("alice").posts.length === 2,
          "explicit window retry",
        );
        const replacement = signatures(run.chain.actor("alice").posts)[1]!;
        expect(replacement.statement).toMatchObject({
          signer: original.statement.signer,
          data: original.statement.data,
          topics: original.statement.topics,
          channel: original.statement.channel,
        });
        expect(replacement.statement.expiry > original.statement.expiry).toBe(
          true,
        );
        expect(
          new Set(run.chain.actor("alice").claims.map((claim) => claim.account))
            .size,
        ).toBe(1);
        await say(bob, "contact-chat", "Alice", "reply after retry");
        expect(await bubble(bob, "window awaiting confirmation").count()).toBe(
          1,
        );
        expect(run.errors).toEqual([]);
      } finally {
        stopA();
        stopB();
        await evidence(run, "contact-retry");
        await run.context.close();
      }
    },
    TIMEOUT,
  );

  test(
    "simulated backup cannot publish its pointer at best and proceeds only after finalized inclusion",
    async () => {
      const run = await offline();
      let claims = 0,
        submits = 0,
        stores = 0;
      const snapshots = new Map<string, string>();
      await run.context.exposeFunction(
        "__truapiBackupNetwork",
        async (op: string, argument: string) => {
          if (op === "claim") {
            claims += 1;
            return JSON.stringify({
              account: `0x${"aa".repeat(32)}`,
              period: Math.floor(NOW / 86_400_000),
              outcome: "registered",
              seq: 0,
              collection: "LitePeople",
              blockHash: CLAIM,
            });
          }
          if (op === "statements") return [];
          if (op === "store") {
            stores += 1;
            const key = bytesToHex(blake2b256(hexToBytes(argument)));
            snapshots.set(key, argument);
            return key;
          }
          if (op === "submit") {
            submits += 1;
            return "new";
          }
          if (op === "fetch") return snapshots.get(argument) ?? null;
          throw new Error(`Unexpected backup operation ${op}`);
        },
      );
      const page = await run.context.newPage();
      page.on("pageerror", (error) => run.errors.push(error.message));
      try {
        await page.clock.install({ time: NOW });
        await page.goto(`${host.origin}/?actor=backup`);
        await ready(page);
        await reveal(page, "#import");
        await page.locator("#import").evaluate((details) => {
          (details as HTMLDetailsElement).open = true;
        });
        await page.fill(
          "#mnemonic",
          "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
        );
        await page.click("#add-wallet");
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await until(
          () => run.chain.actor("backup").backupWaiting === 1,
          "backup finalized gate",
        );
        expect([claims, submits]).toEqual([1, 0]);
        expect(stores).toBe(1);
        expect(run.chain.actor("backup").backupFinal).toBe(0);
        expect(run.chain.actor("backup").rpc).not.toContain(
          "chainHead_v1_storage",
        );
        run.chain.actor("backup").mode = "final";
        await page.clock.fastForward(3_100);
        await until(() => submits === 1, "backup pointer after finality");
        expect(run.chain.actor("backup").backupFinal).toBe(1);
        expect(run.errors).toEqual([]);
      } finally {
        await evidence(run, "backup-finalized");
        await run.context.close();
      }
    },
    TIMEOUT,
  );
});
