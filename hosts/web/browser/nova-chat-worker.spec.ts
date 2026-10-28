/**
 * The Nova chat worker, run by the host page through the ordinary worker
 * loader and the real core, offline: every socket is closed and every
 * request outside this host is refused. The wallet is a BIP-39 test vector
 * and the contact is made up; nothing reaches a network or a person.
 *
 * What runs for real: the engine switch, the dev server's archive of
 * `src/chat/worker/worker.ts`, the archive check, the worker frame, the
 * product's TrUAPI storage and permission prompt, the identity bridge, the
 * v2 draft bridge and the Chat screen. With no People chain, no statement
 * reaches a store, so this checks the boundary, not delivery.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import type { Browser, Frame, Page } from "playwright-core";
import { ChatKeys } from "../src/chat/nova/keys.js";
import { chatRecordKey, type ChatRecord } from "../src/chat/nova/store.js";
import {
  type RunningHost,
  startHost,
  testBrowser,
  closeTools,
  openTools,
  type DevTab,
} from "./support/fixtures.js";

const STEP_MS = 60_000;
const TEST_MS = 240_000;
const WALLET_ID = "nova-worker-wallet";
/** A BIP-39 test vector phrase. It holds nothing. */
const PHRASE =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const PEER = ChatKeys.fromEntropy(
  mnemonicToEntropy(
    "legal winner thank year wave sausage worth useful legal winner thank yellow",
    wordlist,
  ),
  "paseo",
);
const PEER_ACCOUNT = bytesToHex(PEER.identity);
const SELF_ACCOUNT = bytesToHex(
  ChatKeys.fromEntropy(mnemonicToEntropy(PHRASE, wordlist), "paseo").identity,
);

/** The host's own chat record: one made-up contact and two messages. */
const HOST_RECORD: ChatRecord = {
  contacts: [
    {
      account: PEER_ACCOUNT,
      chatKey: bytesToHex(PEER.chatPublic),
      name: "ada.01",
      state: "active",
      messages: [
        {
          id: "m-1",
          direction: "in",
          kind: "text",
          text: "Made-up hello from Ada",
          at: Date.UTC(2026, 9, 20, 9),
          status: "received",
        },
        {
          id: "m-2",
          direction: "out",
          kind: "text",
          text: "Made-up answer",
          at: Date.UTC(2026, 9, 20, 9, 5),
          status: "acknowledged",
        },
      ],
      outbound: null,
      expiry: { request: null, response: null },
      acknowledged: null,
    },
  ],
  requests: [],
  declined: [],
  left: [],
};

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

async function setMenu(
  page: Page,
  open: boolean,
  tab: DevTab = "account",
): Promise<void> {
  if (open) await openTools(page, tab);
  else await closeTools(page);
}

interface WorkerWindow {
  __novaChatWorker?: {
    stage: string;
    reason?: string;
    history: string;
    identity?: string;
    errors: string[];
  };
}

/** Leave an open conversation for the list, if one is open. */
async function backToList(page: Page): Promise<void> {
  const back = page.locator('[aria-label="Back to chats"]');
  if (await back.isVisible()) await back.click();
}

/** The worker frame whose window has `global`, once it appears. */
async function findFrame(page: Page, global: string): Promise<Frame> {
  const until = Date.now() + STEP_MS;
  while (Date.now() < until) {
    for (const frame of page.frames())
      if (
        frame.url().includes("__truapi-worker.html") &&
        (await frame
          .evaluate((name) => name in window, global)
          .catch(() => false))
      )
        return frame;
    await page.waitForTimeout(100);
  }
  throw new Error(`no worker frame has ${global}`);
}

describe("the Nova chat worker", () => {
  test(
    "takes over the wallet's chat from the host's engine through the ordinary worker loader, and gives it back",
    async () => {
      const context = await browser.newContext({
        viewport: { width: 1280, height: 800 },
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
      try {
        const page = await context.newPage();
        await page.goto(host.origin);
        const recordKey = chatRecordKey("paseo", WALLET_ID);
        const seeded = JSON.stringify(HOST_RECORD);
        await page.evaluate(
          ({ id, mnemonic, key, record }) => {
            localStorage.setItem(
              "truapi-web-signing-host:wallets:v1",
              JSON.stringify([{ id, name: "Nova worker test", mnemonic }]),
            );
            localStorage.setItem(key, record);
          },
          { id: WALLET_ID, mnemonic: PHRASE, key: recordKey, record: seeded },
        );
        await page.reload();
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
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

        // By default the host's own engine runs the chat.
        const engine = page.locator("#chat-engine");
        expect(await engine.inputValue()).toBe("host");
        await setMenu(page, false);
        await page.click('#dock [data-destination="chat"]');
        const hostRow = page.locator(
          `[data-conversation="nova/${PEER_ACCOUNT}"]`,
        );
        await hostRow.waitFor();

        // Switch to the worker, copying the host's history once.
        await setMenu(page, true, "more");
        await page.check("#chat-engine-copy");
        await engine.selectOption("worker");
        await setMenu(page, false);
        // The worker asks once to submit statements, as any product would.
        const prompt = page.locator("dialog.prompt", {
          hasText: "Remote permission",
        });
        await prompt.waitFor();
        expect(await prompt.innerText()).toContain("localhost");
        await prompt.locator("button", { hasText: "Allow once" }).click();

        const frame = await findFrame(page, "__novaChatWorker");
        await frame.waitForFunction(
          () =>
            (window as unknown as WorkerWindow).__novaChatWorker?.stage ===
            "running",
        );
        const state = await frame.evaluate(
          () => (window as unknown as WorkerWindow).__novaChatWorker!,
        );
        expect(state).toMatchObject({
          stage: "running",
          history: "copied",
          identity: SELF_ACCOUNT,
        });

        // The host's engine is off: its conversation row is gone, and the
        // worker's room for the same person shows the copied history.
        const workerRow = page.locator(
          `[data-conversation="chat-v2:localhost/${PEER_ACCOUNT}"]`,
        );
        await workerRow.waitFor();
        expect(await hostRow.count()).toBe(0);
        if (process.env.CHAT_SCREENSHOTS)
          await page.screenshot({
            path: `${process.env.CHAT_SCREENSHOTS}/nova-chat-worker-list.png`,
          });
        // The host names the room from its own record: the worker declares
        // the person by account only.
        expect(await workerRow.innerText()).toContain("ada.01");
        const rooms = await frame.evaluate(() =>
          (
            window as unknown as {
              __novaChatWorker: { rooms(): { id: string; title: string }[] };
            }
          ).__novaChatWorker.rooms(),
        );
        expect(rooms.map((room) => room.id)).toEqual([PEER_ACCOUNT]);
        expect(JSON.stringify(rooms)).not.toContain("ada.01");

        // New chat opens the host's picker, which offers the people of the
        // worker's rooms. Cancel changes nothing; choosing Ada opens her room.
        const newChat = page.locator('[aria-label="New chat"]');
        const chooser = page.locator("dialog.prompt", { hasText: "New chat" });
        await newChat.click();
        await chooser.waitFor();
        await chooser.locator("button", { hasText: "Cancel" }).click();
        await chooser.waitFor({ state: "detached" });
        expect(await page.locator(".chat-new-request").count()).toBe(0);
        await newChat.click();
        await chooser.locator("button", { hasText: "ada.01" }).waitFor();
        if (process.env.CHAT_SCREENSHOTS) {
          // The sheet rises in; let it settle.
          await page.waitForTimeout(500);
          await page.screenshot({
            path: `${process.env.CHAT_SCREENSHOTS}/nova-chat-worker-new-chat.png`,
          });
        }
        await chooser.locator("button", { hasText: "ada.01" }).click();
        await chooser.waitFor({ state: "detached" });
        await page
          .locator(".chat-message-in", { hasText: "Made-up hello from Ada" })
          .waitFor();
        expect(await page.locator(".chat-new-request").count()).toBe(0);
        await page.click('[aria-label="Back to chats"]');
        await workerRow.click();
        await page
          .locator(".chat-message-in", { hasText: "Made-up hello from Ada" })
          .waitFor();
        await page
          .locator(".chat-message-out", { hasText: "Made-up answer" })
          .waitFor();
        if (process.env.CHAT_SCREENSHOTS)
          await page.screenshot({
            path: `${process.env.CHAT_SCREENSHOTS}/nova-chat-worker.png`,
          });

        // A send goes to the worker's engine, which owns the message.
        await page.fill(".chat-composer-input", "Made-up offline message");
        await page.press(".chat-composer-input", "Enter");
        await page
          .locator(".chat-message-out", { hasText: "Made-up offline message" })
          .waitFor();
        await frame.waitForFunction(() =>
          (
            window as unknown as {
              __novaChatWorker: {
                snapshot(): { contacts: { messages: { text: string }[] }[] };
              };
            }
          ).__novaChatWorker
            .snapshot()
            .contacts[0].messages.some(
              (message) => message.text === "Made-up offline message",
            ),
        );
        // Offline, the send reaches the core's own statement store client
        // through `statement_store.submit`, which finds no peer to give it to.
        type Snapshot = {
          contacts: { messages: { status: string; problem?: string }[] }[];
        };
        await frame.waitForFunction(
          () =>
            (
              window as unknown as {
                __novaChatWorker: { snapshot(): Snapshot };
              }
            ).__novaChatWorker
              .snapshot()
              .contacts[0].messages.at(-1)?.status === "failed",
        );
        const problem = await frame.evaluate(
          () =>
            (
              window as unknown as {
                __novaChatWorker: { snapshot(): Snapshot };
              }
            ).__novaChatWorker
              .snapshot()
              .contacts[0].messages.at(-1)?.problem,
        );
        expect(problem).toContain("statement-store submit failed");
        await page
          .locator('.chat-message-out[data-status="failed"]', {
            hasText: "Made-up offline message",
          })
          .waitFor();
        await setMenu(page, true, "more");
        expect(
          await page.locator('[data-chat-engine="status"]').innerText(),
        ).toMatch(/Identity lent: [1-9]\d* operations/);
        if (process.env.CHAT_SCREENSHOTS) {
          await page.locator("#developer").scrollIntoViewIfNeeded();
          await page.screenshot({
            path: `${process.env.CHAT_SCREENSHOTS}/nova-chat-worker-menu.png`,
          });
        }
        await setMenu(page, false);

        // Another chat product's worker frame asks for the identity, through
        // the same hello, and is refused: the synthetic product's worker.
        await page.click('#dock [data-destination="chat"]');
        await backToList(page);
        await setMenu(page, true, "more");
        await page.click('#chat-demo [data-action="demo-start"]');
        await setMenu(page, false);
        const other = await findFrame(page, "__syntheticChat");
        const answer = await other.evaluate(
          () =>
            new Promise<unknown>((resolve) => {
              const channel = new MessageChannel();
              channel.port1.onmessage = (event) => resolve(event.data);
              channel.port1.start();
              parent.postMessage(
                {
                  type: "truapi-nova-identity-draft",
                  protocol: "truapi-nova-identity-draft/0",
                },
                location.origin,
                [channel.port2],
              );
              channel.port1.postMessage({
                kind: "call",
                id: 1,
                method: "hello",
                request: {},
              });
            }),
        );
        expect(answer).toEqual({
          kind: "result",
          id: 1,
          ok: false,
          reason: `localhost:${new URL(host.origin).port} may not use this wallet's chat identity.`,
        });
        await setMenu(page, true, "more");
        await page.click('#chat-demo [data-action="demo-stop"]');
        await setMenu(page, false);

        // Switch back: the worker stops, and the host's engine runs again
        // with the history it kept.
        await setMenu(page, true, "more");
        await engine.selectOption("host");
        await setMenu(page, false);
        await page.click('#dock [data-destination="chat"]');
        await backToList(page);
        await hostRow.waitFor();
        await page.waitForFunction(
          () =>
            ![...document.querySelectorAll("iframe")].some((node) =>
              node.src.includes("__truapi-worker.html"),
            ),
        );
        expect(await workerRow.count()).toBe(0);
        // The host's record is the one it had: the copy changed nothing in it.
        const after = JSON.parse(
          (await page.evaluate((key) => localStorage.getItem(key), recordKey))!,
        ) as ChatRecord;
        expect(after.contacts[0].messages.map((m) => m.text)).toEqual([
          "Made-up hello from Ada",
          "Made-up answer",
        ]);
        expect(outside).toEqual([]);
      } finally {
        await context.close();
      }
    },
    TEST_MS,
  );
});
