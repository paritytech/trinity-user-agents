import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { Browser, BrowserContext, Page } from "playwright-core";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { equalBytes } from "../src/platform/codec.js";
import { blake2b256 } from "../src/platform/crypto.js";
import { decodeStatement } from "../src/platform/statement.js";
import {
  type RunningHost,
  reveal,
  startHost,
  testBrowser,
} from "./support/fixtures.js";

/**
 * The host's backup between two browsers of one wallet, on the dev server:
 * each browser profile is its own context, so they share nothing but the
 * simulated network this test runs (`tools/demos/backup-demo.ts`). The core
 * derives the backup account and signs the pointer; the claim, the Bulletin
 * store and the Statement Store are this test's, so nothing reaches the
 * network.
 */

const STEP_MS = 60_000;
const TEST_MS = 240_000;
/** A BIP-39 test vector phrase. It holds nothing. */
const PHRASE =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const OTHER_PHRASE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const SWITCH = "truapi-web-signing-host:backup:switch";
const WALLETS = "truapi-web-signing-host:wallets:v1";
const PRODUCT_KEY = (walletId: string, key: string) =>
  `truapi-web-signing-host:product:${walletId}:truapi:product-storage:v1:17:backup-test.paseo:${key}`;

/** The network every browser of a test reaches, with what was done on it. */
class Network {
  statements: Uint8Array[] = [];
  readonly bulletin = new Map<string, string>();
  claims = 0;
  stores = 0;
  submits = 0;
  /** Bytes Bulletin serves instead of what was stored, when set. */
  tampered: string | null = null;

  counts() {
    return { claims: this.claims, stores: this.stores, submits: this.submits };
  }

  /** The node's channel rule: a newer statement of one signer on one channel replaces the older only with a higher expiry. */
  private submit(encoded: Uint8Array): string {
    this.submits += 1;
    const incoming = decodeStatement(encoded);
    const replaced = this.statements.find((held) => {
      const old = decodeStatement(held);
      return (
        incoming.channel !== null &&
        old.channel !== null &&
        equalBytes(old.channel, incoming.channel) &&
        equalBytes(old.signer, incoming.signer)
      );
    });
    if (
      replaced !== undefined &&
      decodeStatement(replaced).expiry >= incoming.expiry
    )
      return "rejected";
    this.statements = [
      ...this.statements.filter((held) => held !== replaced),
      encoded,
    ];
    return "new";
  }

  answer = async (op: string, argument: string) => {
    switch (op) {
      case "submit":
        return this.submit(hexToBytes(argument as `0x${string}`));
      case "statements": {
        const topic = hexToBytes(argument as `0x${string}`);
        return this.statements
          .filter((held) =>
            decodeStatement(held).topics.some((each) =>
              equalBytes(each, topic),
            ),
          )
          .map((held) => bytesToHex(held));
      }
      case "store": {
        this.stores += 1;
        const key = bytesToHex(
          blake2b256(hexToBytes(argument as `0x${string}`)),
        );
        this.bulletin.set(key, argument);
        return key;
      }
      case "fetch":
        return this.tampered ?? this.bulletin.get(argument) ?? null;
      case "claim":
        this.claims += 1;
        return JSON.stringify({
          account: "0x",
          period: 0,
          outcome: "registered",
          seq: 0,
          collection: "LitePeople",
        });
      default:
        throw new Error(`no ${op} on the simulated network`);
    }
  };
}

let browser: Browser;
let host: RunningHost;

beforeAll(async () => {
  host = await startHost();
  browser = await testBrowser();
}, STEP_MS);

afterAll(async () => {
  await host?.close();
});

/** A browser profile on `network`, with the backup on unless `off`. */
async function profile(
  network: Network,
  options: { off?: boolean } = {},
): Promise<BrowserContext> {
  const context = await browser.newContext();
  context.setDefaultTimeout(STEP_MS);
  await context.exposeFunction("__truapiBackupNetwork", network.answer);
  if (!options.off)
    await context.addInitScript(
      (key) => localStorage.setItem(key, "on"),
      SWITCH,
    );
  return context;
}

async function openHost(context: BrowserContext): Promise<Page> {
  const page = await context.newPage();
  await page.goto(host.origin);
  await page
    .locator("#core-status")
    .filter({ hasText: "Core ready" })
    .waitFor({ state: "attached" });
  return page;
}

/** Import `phrase`, which signs the tab in, and answer the wallet's id in this profile. */
async function signIn(page: Page, phrase = PHRASE): Promise<string> {
  await reveal(page, "#import");
  await page
    .locator("#import")
    .evaluate((details) => ((details as HTMLDetailsElement).open = true));
  await page.fill("#mnemonic", phrase);
  await page.click("#add-wallet");
  await page.locator("#session").filter({ hasNotText: "Signed out" }).waitFor();
  return page.evaluate(
    ([key, phrase]) =>
      (
        JSON.parse(localStorage.getItem(key!) ?? "[]") as {
          id: string;
          mnemonic: string;
        }[]
      ).find((wallet) => wallet.mnemonic === phrase)!.id,
    [WALLETS, phrase],
  );
}

/** Wait for the Activity view to hold a host row reading `text`. */
async function logged(page: Page, text: string): Promise<void> {
  await page
    .locator(`#log .activity-row[data-category="host"]`, { hasText: text })
    .waitFor({ state: "attached" });
}

async function until(check: () => boolean, what: string): Promise<void> {
  for (let waited = 0; !check(); waited += 100) {
    if (waited > STEP_MS) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

const write = (page: Page, key: string, value: string) =>
  page.evaluate(
    ([key, value]) => localStorage.setItem(key!, value!),
    [key, value],
  );
const read = (page: Page, key: string) =>
  page.evaluate((key) => localStorage.getItem(key), key);
const openPrompts = (page: Page) =>
  page.evaluate(() => document.querySelectorAll("dialog[open]").length);
/** The backup writer locks this profile holds and waits for. */
const writerLocks = (page: Page) =>
  page.evaluate(async () => {
    const { held = [], pending = [] } = await navigator.locks.query();
    const writer = (name?: string) =>
      name !== undefined &&
      name.startsWith("truapi-web-signing-host:backup:") &&
      !name.endsWith(":restore");
    return {
      held: held.map((lock) => lock.name).filter(writer),
      pending: pending.map((lock) => lock.name).filter(writer),
    };
  });

/** A first browser that signed in, wrote state, and kept its backup on the network. */
async function firstBrowser(network: Network) {
  const context = await profile(network);
  const page = await openHost(context);
  const walletId = await signIn(page);
  await logged(page, "no backup of this wallet was found");
  await write(page, PRODUCT_KEY(walletId, "note"), "0x68656c6c6f");
  await until(
    () => network.stores === 1 && network.statements.length === 1,
    "the first browser's backup",
  );
  return { context, page, walletId };
}

describe("the host's backup between two browsers of one wallet", () => {
  test(
    "a fresh browser finds and restores the backup under its own wallet id, with no prompt and nothing claimed, stored or posted",
    async () => {
      const network = new Network();
      const first = await firstBrowser(network);
      try {
        const sealed = [...network.bulletin.values()][0]!;
        expect(sealed).not.toContain("68656c6c6f");
        expect(
          new TextDecoder().decode(hexToBytes(sealed as `0x${string}`)),
        ).not.toContain("backup-test");
        const before = network.counts();

        const second = await profile(network);
        try {
          const page = await openHost(second);
          const walletId = await signIn(page);
          expect(walletId).not.toBe(first.walletId);
          await logged(page, "Backup: restored revision 1");
          expect(await read(page, PRODUCT_KEY(walletId, "note"))).toBe(
            "0x68656c6c6f",
          );
          expect(
            await read(page, PRODUCT_KEY(first.walletId, "note")),
          ).toBeNull();
          await new Promise((resolve) => setTimeout(resolve, 3_000));
          expect(network.counts()).toEqual(before);
          expect(await openPrompts(page)).toBe(0);
          expect(await openPrompts(first.page)).toBe(0);
        } finally {
          await second.close();
        }
      } finally {
        await first.context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a browser that already holds the wallet keeps its own state, and only says another browser published",
    async () => {
      const network = new Network();
      const first = await firstBrowser(network);
      const other = await profile(network, { off: true });
      try {
        const page = await openHost(other);
        const walletId = await signIn(page);
        await write(page, PRODUCT_KEY(walletId, "note"), "0x6f776e");
        await page.evaluate((key) => localStorage.setItem(key, "on"), SWITCH);
        await page.reload();
        await page
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await page
          .locator("#session")
          .filter({ hasNotText: "Signed out" })
          .waitFor();
        await logged(page, "another browser published revision 1");
        expect(await read(page, PRODUCT_KEY(walletId, "note"))).toBe(
          "0x6f776e",
        );
      } finally {
        await other.close();
        await first.context.close();
      }
    },
    TEST_MS,
  );

  test(
    "a browser that could not read the backup restores nothing and keeps no backup of its own",
    async () => {
      const network = new Network();
      const first = await firstBrowser(network);
      network.tampered = bytesToHex(new Uint8Array(64));
      const second = await profile(network);
      try {
        const page = await openHost(second);
        const walletId = await signIn(page);
        await logged(page, "Bulletin served bytes that are not the snapshot");
        await logged(page, "not kept in this session");
        expect(await read(page, PRODUCT_KEY(walletId, "note"))).toBeNull();
        const before = network.counts();
        await write(page, PRODUCT_KEY(walletId, "note"), "0x6e6577");
        await new Promise((resolve) => setTimeout(resolve, 5_000));
        expect(network.counts()).toEqual(before);
        expect(await writerLocks(page)).toEqual({ held: [], pending: [] });
      } finally {
        await second.close();
        await first.context.close();
      }
    },
    TEST_MS,
  );

  test(
    "one tab of a browser keeps a wallet's backup, and a wallet switch hands the lock to the new wallet",
    async () => {
      const network = new Network();
      const first = await firstBrowser(network);
      try {
        const [held] = (await writerLocks(first.page)).held;
        expect(held).toBeDefined();
        const tab = await first.context.newPage();
        await tab.goto(host.origin);
        await tab
          .locator("#core-status")
          .filter({ hasText: "Core ready" })
          .waitFor({ state: "attached" });
        await signIn(tab);
        let locks = await writerLocks(tab);
        for (
          let tries = 0;
          locks.pending.length === 0 && tries < 100;
          tries++
        ) {
          await new Promise((resolve) => setTimeout(resolve, 100));
          locks = await writerLocks(tab);
        }
        expect(locks).toEqual({ held: [held!], pending: [held!] });

        await signIn(first.page, OTHER_PHRASE);
        await logged(first.page, "no backup of this wallet was found");
        let after = await writerLocks(tab);
        for (let tries = 0; after.held.length < 2 && tries < 100; tries++) {
          await new Promise((resolve) => setTimeout(resolve, 100));
          after = await writerLocks(tab);
        }
        expect(after.held).toHaveLength(2);
        expect(after.held).toContain(held!);
        expect(after.pending).toEqual([]);
      } finally {
        await first.context.close();
      }
    },
    TEST_MS,
  );
});
