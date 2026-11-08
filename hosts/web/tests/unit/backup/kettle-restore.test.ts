/**
 * A wallet's backup restores Kettle Chat's real MLS state: Kettle's engine as
 * `npm run build:kettle` builds it (`kettle-chat-wasm/`), run over Kettle
 * Chat's own ports and the host's own product storage, with a statement
 * store in memory. Skipped until Kettle is built.
 *
 * A snapshot is taken after every write the first browser makes, as the
 * backup could take it at any moment, and the second browser restores from
 * each of them in turn: after every one, it still sends to the peer and reads
 * the peer's answer. Kettle writes its sealed state as one entry, before each
 * publish (write-ahead), and recovers a direct chat from an older state by a
 * new group.
 */
import { afterEach, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import * as sr25519 from "@scure/sr25519";
import { asProductId, type ProductId } from "@parity/truapi";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import { concat, utf8 } from "../../../src/platform/codec.js";
import { blake2b256 } from "../../../src/platform/crypto.js";
import { signingPayload } from "../../../src/platform/statement.js";
import { HostStorage } from "../../../src/platform/storage.js";
import {
  KETTLE_GLUE,
  KETTLE_WASM,
  KETTLE_WASM_PACKAGE,
} from "../../../src/chat/kettle-chat/route.js";
import {
  conversationPorts,
  type KettleEvent,
  type KettleModule,
  type KettleNode,
} from "../../../src/chat/kettle-chat/ports.js";
import {
  applySnapshot,
  collectSnapshot,
  readSnapshot,
  type Snapshot,
} from "../../../src/backup/snapshot.js";
import { MemoryStorage } from "../../support/test-support.js";

const pkg = new URL(`../../../${KETTLE_WASM_PACKAGE}/`, import.meta.url);
const built = existsSync(new URL(KETTLE_WASM, pkg));

const NETWORK = "paseo";
const PRODUCT: ProductId = asProductId("kettle-chat.paseo");
const INDEX = `0x${"77".repeat(32)}` as HexString;

let kettle: KettleModule;
beforeAll(async () => {
  if (!built) return;
  const glue = await import(new URL(KETTLE_GLUE, pkg).href);
  await glue.default({
    module_or_path: readFileSync(new URL(KETTLE_WASM, pkg)),
  });
  kettle = {
    revision: glue.kettleRevision(),
    protocol: glue.kettleProtocol(),
    start: (account, ports, onEvent, options) =>
      glue.KettleNode.start(account, ports, onEvent, options),
    removeState: (account, ports) =>
      glue.KettleNode.removeState(account, ports),
  };
});

/** A signed statement as TrUAPI hands it over, proof and all. */
interface Signed {
  expiry: bigint;
  channel?: HexString;
  topics: HexString[];
  data?: HexString;
  proof: { tag: "Sr25519"; value: { signer: HexString; signature: HexString } };
}

/**
 * A statement store in memory with the node's channel rule: a newer
 * statement of one signer on one channel replaces the older only with a
 * strictly higher expiry. Every account has an allowance.
 */
class Store {
  held: Signed[] = [];
  private readonly followers = new Set<{
    tag: string;
    topics: string[];
    next: (page: { statements: Signed[] }) => void;
  }>();

  private matches(tag: string, topics: string[], statement: Signed): boolean {
    return tag === "MatchAll"
      ? topics.every((topic) => statement.topics.includes(topic as HexString))
      : topics.some((topic) => statement.topics.includes(topic as HexString));
  }

  submit(statement: Signed): string | null {
    const signer = statement.proof.value.signer;
    const replaced = this.held.find(
      (held) =>
        statement.channel !== undefined &&
        held.proof.value.signer === signer &&
        held.channel === statement.channel,
    );
    if (replaced !== undefined && statement.expiry <= replaced.expiry)
      return "statement-store submit failed: channelPriorityTooLow";
    this.held = [...this.held.filter((held) => held !== replaced), statement];
    for (const { tag, topics, next } of this.followers)
      if (this.matches(tag, topics, statement))
        setTimeout(() => next({ statements: [statement] }), 0);
    return null;
  }

  follow(
    tag: string,
    topics: string[],
    next: (page: { statements: Signed[] }) => void,
  ): () => void {
    const follower = { tag, topics, next };
    this.followers.add(follower);
    const held = this.held.filter((statement) =>
      this.matches(tag, topics, statement),
    );
    setTimeout(() => {
      if (this.followers.has(follower)) next({ statements: held });
    }, 0);
    return () => this.followers.delete(follower);
  }
}

const ok = <T>(value: T) => ({ isOk: () => true, isErr: () => false, value });
const err = (error: unknown) => ({
  isOk: () => false,
  isErr: () => true,
  error,
});

/** The core's product storage key for `key` of {@link PRODUCT}. */
const storageKey = (key: string) =>
  `truapi:product-storage:v1:${utf8(PRODUCT).length}:${PRODUCT}:${key}`;

/**
 * One browser's Kettle Chat for one wallet: Kettle Chat's ports over a
 * TrUAPI client whose core signs as the wallet's conversation account and
 * whose storage is the host's own, in `storage` under `walletId`.
 */
function browser(
  seed: number,
  walletId: string,
  storage: MemoryStorage,
  store: Store,
) {
  const wallet = new Uint8Array(32).fill(seed);
  const secret = sr25519.secretFromSeed(
    blake2b256(concat(wallet, hexToBytes(INDEX))),
  );
  const account = bytesToHex(sr25519.getPublicKey(secret)) as HexString;
  const host = new HostStorage(storage, () => () => {});
  host.useWallet(walletId, NETWORK);
  const client = {
    localStorage: {
      read: async ({ key }: { key: string }) => {
        const value = await host.product.read(storageKey(key));
        return ok({
          value: value === undefined ? undefined : bytesToHex(value),
        });
      },
      write: async ({ key, value }: { key: string; value: HexString }) => {
        await host.product.write(storageKey(key), hexToBytes(value));
        return ok(undefined);
      },
      clear: async ({ key }: { key: string }) => {
        await host.product.clear(storageKey(key));
        return ok(undefined);
      },
    },
    entropy: {
      derive: async ({ context }: { context: HexString }) =>
        ok({
          entropy: bytesToHex(blake2b256(concat(wallet, hexToBytes(context)))),
        }),
    },
    statementStore: {
      createProof: async ({
        statement,
      }: {
        statement: Omit<Signed, "proof">;
      }) => {
        const payload = signingPayload({
          expiry: statement.expiry,
          channel:
            statement.channel === undefined
              ? null
              : hexToBytes(statement.channel),
          topics: statement.topics.map((topic) => hexToBytes(topic)),
          data:
            statement.data === undefined
              ? new Uint8Array()
              : hexToBytes(statement.data),
        });
        return ok({
          proof: {
            tag: "Sr25519",
            value: {
              signer: account,
              signature: bytesToHex(sr25519.sign(secret, payload)),
            },
          },
        });
      },
      submit: async (statement: Signed) => {
        const refused = store.submit(statement);
        return refused === null
          ? ok(undefined)
          : err({ tag: "Domain", value: { reason: refused } });
      },
      subscribe: ({
        request,
      }: {
        request: { tag: string; value: string[] };
      }) => ({
        subscribe: (observer: {
          next: (page: { statements: Signed[] }) => void;
        }) => {
          const stop = store.follow(request.tag, request.value, observer.next);
          return { unsubscribe: stop };
        },
      }),
    },
  };
  const ports = conversationPorts({
    client: client as never,
    productId: PRODUCT,
    index: INDEX,
    allow: async () => null,
    report: () => {},
  });
  return { account, ports, storage };
}

const running: KettleNode[] = [];
afterEach(async () => {
  for (const node of running.splice(0)) {
    await node.stop();
    node.free();
  }
});

async function start(
  side: ReturnType<typeof browser>,
): Promise<{ node: KettleNode; texts: string[] }> {
  const texts: string[] = [];
  const node = await kettle.start(
    side.account,
    side.ports,
    (event: KettleEvent) => {
      if (event.type === "message") texts.push(String(event.text));
    },
    { fast: true },
  );
  running.push(node);
  return { node, texts };
}

async function until(check: () => boolean, what: string): Promise<void> {
  for (let waited = 0; !check(); waited += 25) {
    if (waited > 20_000) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

async function stop(node: KettleNode): Promise<void> {
  running.splice(running.indexOf(node), 1);
  await node.stop();
  node.free();
}

describe("a restored browser runs Kettle Chat's MLS state", () => {
  test.skipIf(!built)(
    "after a snapshot taken at any write of the first browser, the second still sends to the peer and reads its answer",
    async () => {
      const store = new Store();
      const cuts: Snapshot[] = [];
      const first = new MemoryStorage();
      const write = first.setItem.bind(first);
      first.setItem = (key, value) => {
        write(key, value);
        cuts.push(collectSnapshot(first, { network: NETWORK, walletId: "wa" }));
      };
      const a = browser(0xa1, "wa", first, store);
      const c = browser(0xc3, "wc", new MemoryStorage(), store);
      const nodeA = await start(a);
      const peer = await start(c);

      await nodeA.node.sendText(c.account, "a1");
      await until(() => peer.texts.includes("a1"), "the peer to read a1");
      await peer.node.sendText(a.account, "c1");
      await until(() => nodeA.texts.includes("c1"), "a to read c1");
      await nodeA.node.sendText(c.account, "a2");
      await until(() => peer.texts.includes("a2"), "the peer to read a2");
      const group = await nodeA.node.directGroup(c.account);
      expect(group).not.toBeNull();
      await nodeA.node.saveState();
      await stop(nodeA.node);
      await peer.node.saveState();
      await stop(peer.node);
      const peerState = new Map(
        (c.storage as MemoryStorage)
          .keys()
          .map((key) => [key, c.storage.getItem(key)!]),
      );
      const held = [...store.held];

      // Every write the first browser made is a moment a snapshot could be
      // taken. Each restore starts from where the first browser and the peer
      // stopped, with the second browser at that moment's snapshot.
      const withState = cuts.filter((cut) => cut.products.length > 0);
      expect(withState.length).toBeGreaterThan(2);
      for (const [index, cut] of withState.entries()) {
        const network = new Store();
        network.held = [...held];
        const peerStorage = new MemoryStorage();
        for (const [key, value] of peerState) peerStorage.setItem(key, value);
        const peerAgain = await start(
          browser(0xc3, "wc", peerStorage, network),
        );
        const second = new MemoryStorage();
        applySnapshot(
          second,
          { network: NETWORK, walletId: "wb" },
          readSnapshot(JSON.parse(JSON.stringify(cut))),
        );
        const b = browser(0xa1, "wb", second, network);
        expect(b.account).toBe(a.account);
        const nodeB = await start(b);
        // The last snapshot holds the group as the first browser left it:
        // the second continues it, rather than making a new one.
        if (index === withState.length - 1)
          expect(await nodeB.node.directGroup(c.account)).toBe(group);
        await nodeB.node.sendText(c.account, `b${index}`);
        await until(
          () => peerAgain.texts.includes(`b${index}`),
          `the peer to read b${index} after restoring cut ${index} of ${withState.length}`,
        );
        await peerAgain.node.sendText(b.account, `c-b${index}`);
        await until(
          () => nodeB.texts.includes(`c-b${index}`),
          `the restored browser to read c-b${index} after cut ${index} of ${withState.length}`,
        );
        await stop(nodeB.node);
        await stop(peerAgain.node);
      }
    },
    300_000,
  );
});
