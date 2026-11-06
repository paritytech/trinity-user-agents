import { describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { encodeSigned } from "../../../src/platform/statement.js";
import { MemoryWorld } from "../../../fixtures/invitations/shared-store.js";
import { LocalStatementNode } from "../../../fixtures/statements/local-statement-node.js";

const NOW = 1_800_000_000_000;
const secret = sr25519.secretFromSeed(new Uint8Array(32).fill(3));
const signer = sr25519.getPublicKey(secret);
const topic = (value: number) => new Uint8Array(32).fill(value);
const expiry = (seconds: number, priority = 0) =>
  (BigInt(seconds) << 32n) | BigInt(priority);
const later = NOW / 1000 + 3600;

const statement = (options: {
  channel?: number;
  topics: number[];
  priority: number;
  data?: number;
}) =>
  bytesToHex(
    encodeSigned(
      {
        expiry: expiry(later, options.priority),
        channel: options.channel === undefined ? null : topic(options.channel),
        topics: options.topics.map(topic),
        data: new Uint8Array([options.data ?? options.priority]),
      },
      signer,
      (payload) => sr25519.sign(secret, payload),
    ),
  );

/** A node of `world` for one document, with what its subscription to `filter` received. */
async function subscribed(world: MemoryWorld, filter: object) {
  const node = new LocalStatementNode({
    store: world.store(),
    network: "test",
    now: () => NOW,
  });
  const connection = node.connect(() =>
    Promise.reject(new Error("no real node here")),
  );
  const notified: string[][] = [];
  const reader = connection.responses()[Symbol.asyncIterator]();
  void (async () => {
    for (;;) {
      const next = await reader.next();
      if (next.done === true) return;
      const message = JSON.parse(next.value) as {
        method?: string;
        params?: { result: { data: { statements: string[] } } };
      };
      if (message.method === "statement_statement")
        notified.push(message.params!.result.data.statements);
    }
  })();
  connection.send(
    JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "statement_subscribeStatement",
      params: [filter],
    }),
  );
  await Bun.sleep(0);
  return { node, connection, notified };
}

describe("the simulated Statement Store", () => {
  test("a subscription in another document gets what is stored, then what arrives on its topics", async () => {
    const world = new MemoryWorld();
    const writer = new LocalStatementNode({
      store: world.store(),
      network: "test",
      now: () => NOW,
    });
    const stored = statement({ topics: [1], priority: 1 });
    expect(writer.submit(stored)).toEqual({ status: "new" });
    expect(writer.submit(stored)).toEqual({ status: "known" });
    const { notified } = await subscribed(world, {
      matchAny: [bytesToHex(topic(1))],
    });
    const arriving = statement({ topics: [1, 2], priority: 2 });
    writer.submit(arriving);
    writer.submit(statement({ topics: [3], priority: 3 }));
    await Bun.sleep(0);
    expect(notified).toEqual([[stored], [arriving]]);
  });

  test("keeps one statement per channel of an account, the higher expiry winning", () => {
    const node = new LocalStatementNode({
      store: new MemoryWorld().store(),
      network: "test",
      now: () => NOW,
    });
    expect(
      node.submit(statement({ channel: 9, topics: [1], priority: 2 })),
    ).toEqual({ status: "new" });
    expect(
      node.submit(statement({ channel: 9, topics: [1], priority: 1 })),
    ).toEqual({
      status: "rejected",
      reason: "channelPriorityTooLow",
    });
    expect(
      node.submit(statement({ channel: 9, topics: [1], priority: 3 })),
    ).toEqual({ status: "new" });
  });

  test("keeps two statements per account, evicting the lowest expiry for a higher one", () => {
    const world = new MemoryWorld();
    const node = new LocalStatementNode({
      store: world.store(),
      network: "test",
      now: () => NOW,
    });
    node.submit(statement({ channel: 1, topics: [1], priority: 5 }));
    node.submit(statement({ channel: 2, topics: [1], priority: 6 }));
    expect(
      node.submit(statement({ channel: 3, topics: [1], priority: 4 })),
    ).toEqual({
      status: "rejected",
      reason: "accountFull",
    });
    expect(
      node.submit(statement({ channel: 3, topics: [1], priority: 7 })),
    ).toEqual({ status: "new" });
    const kept = [...world.items.values()].map((value) =>
      hexToBytes((JSON.parse(value) as { encoded: `0x${string}` }).encoded).at(
        -1,
      ),
    );
    expect(kept.sort()).toEqual([6, 7]);
  });

  test("refuses a statement whose proof does not sign it", () => {
    const node = new LocalStatementNode({
      store: new MemoryWorld().store(),
      network: "test",
      now: () => NOW,
    });
    const forged = bytesToHex(
      encodeSigned(
        {
          expiry: expiry(later),
          channel: null,
          topics: [topic(1)],
          data: new Uint8Array(1),
        },
        signer,
        () => new Uint8Array(64),
      ),
    );
    expect(node.submit(forged)).toEqual({
      status: "invalid",
      reason: "badProof",
    });
  });
});
