/**
 * LIVE AND SUBMITTING, run only with TRUAPI_LIVE=1 and TRUAPI_LIVE_SUBMIT=1:
 * Kettle Chat between the public test accounts `testalice.10` and
 * `testbob.10` on Paseo Next v2, each in its own browser context, through
 * the host's Chat UI. It needs a Chromium and Kettle's WebAssembly
 * (`npm run build:kettle`).
 *
 * What it spends: the request and the reply each claim one notification
 * slot of the sender's personhood and post one sealed statement; each
 * side's conversation account claims one general Statement Store slot for
 * the UTC day, and Kettle posts its keys statement and its outbox under it.
 *
 * Nothing is simulated: the wallet session, the request service's network
 * delivery over the day inbox, the core's claims, `createProof` and
 * `submit`, and People's RPC node. The test then reads the conversation's
 * statements back from that node itself, on a connection of its own, and
 * checks their proofs, their signers and that no message text is in them.
 * A failed run keeps its evidence (`browser/support/live.ts`).
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { blake3 } from "@noble/hashes/blake3.js";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { Browser, BrowserContext, Frame, Page } from "playwright-core";
import {
  type RunningHost,
  closeTools,
  openTools,
  startHost,
  testBrowser,
  useChatProducts,
} from "./support/fixtures.js";
import {
  LIVE_STEP_MS,
  keepEvidence,
  openInbox,
  ready,
  sendChatRequest,
} from "./support/live.js";
import { NETWORKS } from "../src/platform/network-config.js";
import {
  decodeStatement,
  statementHash,
  verifyStatement,
} from "../src/platform/statement.js";
import type { KettleSubmission } from "../src/chat/kettle-chat/ports.js";
import {
  KETTLE_WASM,
  KETTLE_WASM_PACKAGE,
} from "../src/chat/kettle-chat/route.js";

const LIVE =
  process.env.TRUAPI_LIVE === "1" && process.env.TRUAPI_LIVE_SUBMIT === "1";
const BUILT = existsSync(
  new URL(`../${KETTLE_WASM_PACKAGE}/${KETTLE_WASM}`, import.meta.url),
);
const suite = LIVE && BUILT ? describe : describe.skip;
const TEST_MS = 1_800_000;
const PRODUCT = "kettle-chat.paseo";
const PEOPLE_RPC = NETWORKS.paseo.chains.people.rpc;

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  if (!LIVE || !BUILT) return;
  host = await startHost();
  browser = await testBrowser();
}, LIVE_STEP_MS);

// Bounded like the setup: a dev server that does not close fails the run
// here, instead of holding it until the 30 min test timeout.
afterAll(async () => {
  await host?.close();
}, LIVE_STEP_MS);

interface KettleWorker {
  stage: string;
  reason?: string;
  errors: string[];
  submits: Record<string, number>;
  submissions: KettleSubmission[];
  trace: string[];
  kettle?: { revision: string; protocol: number };
  record(): string | null;
}

interface StoredConversation {
  status: string;
  account?: string;
  peer?: string;
  group?: string;
}

const kettleWorker = (frame: Frame) =>
  frame.evaluate(() => {
    const worker = (window as unknown as { __kettleChatWorker?: KettleWorker })
      .__kettleChatWorker;
    return worker === undefined
      ? null
      : {
          stage: worker.stage,
          reason: worker.reason,
          errors: worker.errors,
          submits: worker.submits,
          submissions: worker.submissions,
          trace: worker.trace,
          kettle: worker.kettle,
          record: worker.record(),
        };
  });

async function workerFrame(page: Page): Promise<Frame> {
  const until = Date.now() + LIVE_STEP_MS;
  let last = "";
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (frame.url().includes("__truapi-worker.html")) {
        const worker = await kettleWorker(frame).catch(() => null);
        if (worker?.stage === "running") return frame;
        if (worker !== null) last = `${worker.stage} ${worker.reason ?? ""}`;
      }
    await page.waitForTimeout(250);
  }
  throw new Error(`Kettle Chat's worker did not start: ${last}`);
}

/** Run Kettle Chat alone in this tab, then sign in as the test account. */
async function walletTab(
  context: BrowserContext,
  account: "alice" | "bob",
): Promise<Page> {
  const page = await context.newPage();
  await page.goto(host.origin);
  await ready(page);
  await useChatProducts(page, { contact: false, kettle: true });
  await ready(page);
  await signedIn(page, account);
  return page;
}

async function signedIn(page: Page, account: "alice" | "bob"): Promise<void> {
  const name = account === "alice" ? "Alice" : "Bob";
  await openTools(page, "account");
  await page.selectOption("#wallet", `test-account:${account}`);
  await page.click("#sign-in");
  await page
    .locator("#session")
    .filter({ hasText: `Signed in as ${name} (test account)` })
    .waitFor({ state: "attached" });
  await closeTools(page);
}

/** After a reload the session comes back by itself, named by its username or as the test account. */
async function restored(page: Page, account: "alice" | "bob"): Promise<void> {
  const name = account === "alice" ? "Alice" : "Bob";
  await page
    .locator("#session")
    .filter({
      hasText: new RegExp(
        `Signed in as (test${account}\\.10|${name} \\(test account\\))`,
      ),
    })
    .waitFor({ state: "attached" });
}

interface Settled {
  account: string;
  claimBlock?: string;
  seq: number;
  collection: string;
  finalizedBlock: string;
  grant: { maxCount: number; maxSize: number };
  waitedMs: number;
  reads: number;
  at: string;
}

/** What the host's finality guard saw for each conversation account it let post. */
const allowancesOf = (page: Page) =>
  page.evaluate(
    () =>
      (window as unknown as { __kettleAllowances?: unknown[] })
        .__kettleAllowances ?? [],
  ) as Promise<Settled[]>;

/** The core's question before the product's first statement: always allow it. */
async function allowStatements(page: Page): Promise<void> {
  const prompt = page.locator("dialog.prompt", { hasText: "StatementSubmit" });
  await prompt.waitFor();
  expect(await prompt.innerText()).toContain(PRODUCT);
  await prompt.getByRole("button", { name: "Always allow" }).click();
  await prompt.waitFor({ state: "detached" });
}

const bubble = (page: Page, text: string) =>
  page.locator(".chat-bubble-text", { hasText: text });

async function write(page: Page, text: string): Promise<void> {
  await page.fill(".chat-composer-input", text);
  await page.press(".chat-composer-input", "Enter");
}

const row = (page: Page) =>
  page.locator(`[data-conversation^="chat-v2:${PRODUCT}/"]`).first();

async function openChat(page: Page): Promise<void> {
  await page.click('#dock [data-destination="chat"]');
  const back = page.locator('#chat-screen button[aria-label="Back to chats"]');
  if ((await back.count()) > 0) await back.click();
  await page.click('#chat-tabs [data-chat-tab="chats"]');
  await row(page).click();
}

const topic = (context: string, material: string) =>
  bytesToHex(
    blake3(hexToBytes(material as `0x${string}`), {
      context: new TextEncoder().encode(context),
    }),
  );

/** What People's RPC node holds on `topics`, read on a connection of this test's own. */
async function nodeStatements(topics: string[]): Promise<string[]> {
  const socket = new WebSocket(PEOPLE_RPC);
  const statements: string[] = [];
  try {
    await read(socket, topics, statements);
  } finally {
    socket.close();
  }
  return statements;
}

/** Subscribe on `socket` and collect what the node sends, until it has sent what it holds. */
function read(
  socket: WebSocket,
  topics: string[],
  statements: string[],
): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const timer = setTimeout(resolve, 15_000);
    socket.onerror = () => {
      clearTimeout(timer);
      reject(new Error(`${PEOPLE_RPC} could not be reached`));
    };
    socket.onopen = () =>
      socket.send(
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "statement_subscribeStatement",
          params: [{ matchAny: topics }],
        }),
      );
    socket.onmessage = (event) => {
      const message = JSON.parse(String(event.data)) as {
        method?: string;
        error?: { message: string };
        params?: {
          result: {
            data?: { statements: string[]; remaining?: number };
          };
        };
      };
      if (message.error !== undefined) {
        clearTimeout(timer);
        reject(new Error(message.error.message));
      }
      const page = message.params?.result.data;
      if (message.method !== "statement_statement" || page === undefined)
        return;
      statements.push(...page.statements);
      if ((page.remaining ?? 0) === 0) {
        clearTimeout(timer);
        setTimeout(resolve, 1500);
      }
    };
  });
}

const derived = (context: string, material: Uint8Array) =>
  bytesToHex(blake3(material, { context: new TextEncoder().encode(context) }));
const KEYS_CHANNEL = derived("kettle v1 prekey channel", new Uint8Array());
const outboxChannel = (group: string) =>
  derived("kettle v3 group outbox channel", hexToBytes(group as `0x${string}`));

/** What the node holds on `topics`, each by the hash the store names it by. */
async function readback(topics: string[]) {
  const at = new Date().toISOString();
  return (await nodeStatements(topics)).map((encoded) => {
    const bytes = hexToBytes(encoded as `0x${string}`);
    const statement = decodeStatement(bytes);
    return {
      at,
      hash: bytesToHex(statementHash(bytes)),
      signer: bytesToHex(statement.signer).toLowerCase(),
      channel:
        statement.channel === null ? null : bytesToHex(statement.channel),
      topics: statement.topics.map((each) => bytesToHex(each)),
      dataBytes: statement.data.length,
      proofOk: verifyStatement(statement),
      expiresAt: new Date(Number(statement.expiry >> 32n) * 1000).toISOString(),
      encoded,
    };
  });
}

type Held = Awaited<ReturnType<typeof readback>>;

/**
 * Where an account's statement on `channel` stands at a read: the node holds
 * the latest submission accepted before the read began (or a newer one),
 * only an older one (a later one was lost or not yet seen), or none.
 */
function standing(
  held: Held,
  submissions: KettleSubmission[],
  account: string,
  channel: string,
) {
  const readAt = Date.parse(held[0]?.at ?? new Date().toISOString());
  const sent = submissions.filter(
    (entry) =>
      entry.signer.toLowerCase() === account &&
      entry.channel === channel &&
      entry.outcome === "new",
  );
  const found = held.filter(
    (entry) => entry.signer === account && entry.channel === channel,
  );
  const latestBefore = sent.findLastIndex(
    (entry) => Date.parse(entry.at) <= readAt,
  );
  const positions = found.map((entry) =>
    sent.findIndex((each) => each.hash === entry.hash),
  );
  return {
    account,
    channel,
    accepted: sent.map(({ at, hash }) => ({ at, hash })),
    held: found.map((entry) => entry.hash),
    state:
      found.length === 0
        ? "missing"
        : positions.some(
              (position) => position >= latestBefore && position >= 0,
            )
          ? "latest"
          : positions.some((position) => position >= 0)
            ? "older"
            : "unknown",
  };
}

const utf8Hex = (text: string) => Buffer.from(text, "utf8").toString("hex");

suite("Kettle Chat between test accounts on Paseo Next v2", () => {
  test(
    "invite, accept, write both ways encrypted, reload, write again; the People node holds the statements",
    async () => {
      const stamp = new Date().toISOString().slice(11, 19);
      const texts = {
        welcome: `Kettle live ${stamp}`,
        bob: `Bob over MLS ${stamp}`,
        alice: `Alice over MLS ${stamp}`,
        after: `After the reload ${stamp}`,
      };
      const aliceContext = await browser.newContext({
        viewport: { width: 1280, height: 860 },
      });
      const bobContext = await browser.newContext({
        viewport: { width: 1280, height: 860 },
      });
      for (const context of [aliceContext, bobContext])
        context.setDefaultTimeout(LIVE_STEP_MS);
      const errors: string[] = [];
      const pages: Record<string, Page> = {};
      const steps: { step: string; at: string }[] = [];
      const checks: Record<string, unknown> = {};
      const mark = (step: string) =>
        steps.push({ step, at: new Date().toISOString() });
      try {
        const alice = await walletTab(aliceContext, "alice");
        const bob = await walletTab(bobContext, "bob");
        pages.alice = alice;
        pages.bob = bob;
        for (const page of [alice, bob])
          page.on("pageerror", (error) => errors.push(error.message));
        const aliceWorker = await workerFrame(alice);
        const bobFirstWorker = await workerFrame(bob);
        const loaded = (await kettleWorker(aliceWorker))?.kettle;
        expect([loaded?.revision, loaded?.protocol]).toEqual([
          "4d0764d526e5e32dfc611323e5fa522651f864e6",
          6,
        ]);
        // The finality guard must be on in both tabs before anything is spent.
        for (const page of [alice, bob])
          expect(
            await page.evaluate(() =>
              Array.isArray(
                (window as unknown as { __kettleAllowances?: unknown })
                  .__kettleAllowances,
              ),
            ),
          ).toBe(true);
        mark("both signed in, Kettle Chat running, finality guard on");

        await sendChatRequest(alice, "testbob.10", texts.welcome);
        mark("request sent");

        await openInbox(bob);
        const request = bob
          .locator(".notification-row", { hasText: texts.welcome })
          .first();
        await request.waitFor();
        mark("request in Bob's Inbox");
        await request.locator('[data-action="notification-accept"]').click();
        await bob
          .locator("dialog.prompt", { hasText: "Accept chat request" })
          .getByRole("button", { name: "Accept" })
          .click();
        await allowStatements(bob);
        mark("Bob accepted; his node posts its keys");

        await alice
          .locator(".chat-system", {
            hasText: "They accepted your chat request",
          })
          .waitFor();
        mark("Alice has the authenticated reply");
        await allowStatements(alice);

        // The first message shows at once; the chat is set up once the
        // finality guard let her node start.
        await bubble(alice, texts.welcome).waitFor();
        await alice
          .locator(".chat-system", { hasText: "The encrypted chat is set up" })
          .waitFor();
        mark("Alice's node started after the guard; it makes the group");

        await openChat(bob);
        await bob
          .locator(".chat-system", { hasText: "The encrypted chat is set up" })
          .waitFor();
        await bubble(bob, texts.welcome).waitFor();
        mark("Bob joined the MLS group and read the first message");

        await write(bob, texts.bob);
        await bubble(alice, texts.bob).waitFor();
        mark("Alice read Bob's message");
        await write(alice, texts.alice);
        await bubble(bob, texts.alice).waitFor();
        mark("Bob read Alice's message");

        // Retention: each account's first and only keys statement, read
        // back from the node at least 65 s after it was accepted, before any
        // republish could replace it; and each outbox, latest or not.
        const aliceRecord = JSON.parse(
          (await kettleWorker(aliceWorker))!.record!,
        ) as { conversations: StoredConversation[] };
        const live = aliceRecord.conversations.find(
          (entry) => entry.status === "connected" && entry.group !== undefined,
        )!;
        const accounts = {
          alice: live.account!.toLowerCase(),
          bob: live.peer!.toLowerCase(),
        };
        const group = live.group!;
        const allowances = {
          alice: await allowancesOf(alice),
          bob: await allowancesOf(bob),
        };
        Object.assign(checks, { accounts, group, allowances });
        const subs = async () => ({
          alice: (await kettleWorker(aliceWorker))!.submissions,
          bob: (await kettleWorker(bobFirstWorker))!.submissions,
        });
        const keysOf = (list: KettleSubmission[]) =>
          list.filter(
            (entry) =>
              entry.channel === KEYS_CHANNEL && entry.outcome === "new",
          );
        const before = await subs();
        const lastKeys = Math.max(
          ...[...keysOf(before.alice), ...keysOf(before.bob)].map((entry) =>
            Date.parse(entry.at),
          ),
        );
        const pause = lastKeys + 65_000 - Date.now();
        if (pause > 0) await alice.waitForTimeout(pause);
        const keysHeld = await readback([
          topic("kettle v1 prekey topic", accounts.alice),
          topic("kettle v1 prekey topic", accounts.bob),
        ]);
        const outboxHeld = await readback([
          topic("kettle v3 group topic", group),
        ]);
        const after = await subs();
        const sides = ["alice", "bob"] as const;
        const retention = {
          keys: sides.map((side) => ({
            side,
            submittedBefore: keysOf(before[side]).map(({ at, hash }) => ({
              at,
              hash,
            })),
            submittedByRead: keysOf(after[side]).length,
            ...standing(keysHeld, after[side], accounts[side], KEYS_CHANNEL),
          })),
          outbox: sides.map((side) => ({
            side,
            ...standing(
              outboxHeld,
              after[side],
              accounts[side],
              outboxChannel(group),
            ),
          })),
          keysHeld,
          outboxHeld,
        };
        Object.assign(checks, { retention, submissions: after });
        mark("read the keys back at least 65 s after they were accepted");
        for (const side of sides) {
          expect(keysOf(after[side])).toHaveLength(1);
          const firstSubmit = Date.parse(after[side][0]!.at);
          const settled = allowances[side].find(
            (entry) => entry.account.toLowerCase() === accounts[side],
          );
          expect(
            settled !== undefined && Date.parse(settled.at) <= firstSubmit,
          ).toBe(true);
        }
        expect(retention.keys.map((entry) => entry.state)).toEqual([
          "latest",
          "latest",
        ]);
        expect(retention.outbox.map((entry) => entry.state)).toEqual([
          "latest",
          "latest",
        ]);

        await bob.reload();
        await ready(bob);
        await restored(bob, "bob");
        const bobWorker = await workerFrame(bob);
        await openChat(bob);
        await bubble(bob, texts.alice).waitFor();
        mark("Bob reloaded with his history");
        await write(alice, texts.after);
        await bubble(bob, texts.after).waitFor();
        await bob.waitForTimeout(5000);
        expect(await bubble(bob, texts.after).count()).toBe(1);
        mark("Bob read the message after his reload, once");

        // After the reload: everything the node holds of the conversation.
        const finalSubs = {
          alice: (await kettleWorker(aliceWorker))!.submissions,
          bob: [...after.bob, ...(await kettleWorker(bobWorker))!.submissions],
        };
        const finalHeld = await readback([
          topic("kettle v3 group topic", group),
          topic("kettle v1 prekey topic", accounts.alice),
          topic("kettle v1 prekey topic", accounts.bob),
        ]);
        Object.assign(checks, {
          final: {
            keys: sides.map((side) => ({
              side,
              ...standing(
                finalHeld,
                finalSubs[side],
                accounts[side],
                KEYS_CHANNEL,
              ),
            })),
            outbox: sides.map((side) => ({
              side,
              ...standing(
                finalHeld,
                finalSubs[side],
                accounts[side],
                outboxChannel(group),
              ),
            })),
            held: finalHeld,
          },
          submissions: finalSubs,
        });
        expect(finalHeld.every((entry) => entry.proofOk)).toBe(true);
        for (const text of Object.values(texts))
          expect(
            finalHeld.some((entry) => entry.encoded.includes(utf8Hex(text))),
          ).toBe(false);
        mark("read everything back after the reload");

        const report = {
          environment: {
            network: NETWORKS.paseo.displayName,
            peopleRpc: PEOPLE_RPC,
            kettle: (await kettleWorker(aliceWorker))?.kettle,
          },
          steps,
          checks,
          submits: {
            alice: (await kettleWorker(aliceWorker))?.submits,
            bob: (await kettleWorker(bobWorker))?.submits,
          },
          trace: {
            alice: (await kettleWorker(aliceWorker))?.trace,
            bob: (await kettleWorker(bobWorker))?.trace,
          },
          workerErrors: {
            alice: (await kettleWorker(aliceWorker))?.errors,
            bob: (await kettleWorker(bobWorker))?.errors,
          },
        };
        console.log(
          `LIVE REPORT ${await keepEvidence("kettle-chat-live", pages, report)}`,
        );
        console.log(JSON.stringify(report, null, 2));
        expect(errors).toEqual([]);
      } catch (error) {
        const workers: Record<string, unknown> = {};
        for (const [label, page] of Object.entries(pages))
          for (const frame of page.frames())
            if (frame.url().includes("__truapi-worker.html"))
              workers[label] = await kettleWorker(frame).catch(String);
        console.log(
          `evidence: ${await keepEvidence("kettle-chat-live-failed", pages, {
            steps,
            errors,
            error: String(error),
            checks,
            workers,
          })}`,
        );
        throw error;
      } finally {
        await aliceContext.close();
        await bobContext.close();
      }
    },
    TEST_MS,
  );
});
