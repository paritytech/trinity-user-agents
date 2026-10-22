import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import {
  decodeConsumer,
  decodeLitePersonKey,
  decodePersonKey,
  decodeRingPosition,
  mapKey,
  readPeopleChain,
  readStorageValues,
  type RingMembership,
} from "./people-chain.js";

const CONSUMERS =
  "2111e0df19de9563b58301e5f7e0074311ab70ef474cf12409dfe509d5efe3b2";
const NO_RINGS: RingMembership[] = [
  { collection: "LitePeople", lookup: { tag: "NoKey" } },
  { collection: "People", lookup: { tag: "NoKey" } },
];
const ACCOUNT = new Uint8Array(32).fill(1);
const OTHER = new Uint8Array(32).fill(2);
const ACCOUNT_HEX = bytesToHex(ACCOUNT);
const OTHER_HEX = bytesToHex(OTHER);

const text = (value: string) => [
  value.length << 2,
  ...new TextEncoder().encode(value),
];
const u64 = (value: number) => {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), true);
  return [...out];
};

/** A `ConsumerInfo` value as the chain stores it. */
function consumer(options: {
  full?: string;
  lite: string;
  person?: { demoted: boolean };
}): Uint8Array {
  return Uint8Array.from([
    ...new Uint8Array(65),
    ...(options.full === undefined ? [0] : [1, ...text(options.full)]),
    ...text(options.lite),
    ...(options.person
      ? [
          1,
          ...new Uint8Array(32).fill(9),
          ...u64(1_700_000_000),
          options.person.demoted ? 1 : 0,
        ]
      : [0]),
  ]);
}

describe("storage keys", () => {
  // The prefix is twox128 of the pallet and item, the account follows its own
  // blake2_128 hash. The expected key was computed with Python's blake2b.
  test("puts the hash and then the account after the map prefix", () => {
    expect(mapKey(CONSUMERS, ACCOUNT)).toBe(
      `0x${CONSUMERS}c035f853fcd0f0589e30c9e2dc1a0f57${"01".repeat(32)}`,
    );
  });
});

describe("Consumers record", () => {
  // The username a local session is activated with comes from this record,
  // so a Lite record must decode, not read as unreadable.
  test("reads a Lite account with no full name", () => {
    expect(decodeConsumer(consumer({ lite: "alice.07" }))).toEqual({
      fullUsername: null,
      liteUsername: "alice.07",
      credibility: { tag: "Lite" },
    });
  });

  test("reads a person's names and credibility", () => {
    const record = decodeConsumer(
      consumer({ full: "alice", lite: "alice.07", person: { demoted: true } }),
    );
    expect(record).toEqual({
      fullUsername: "alice",
      liteUsername: "alice.07",
      credibility: {
        tag: "Person",
        alias: `0x${"09".repeat(32)}`,
        lastUpdate: 1_700_000_000n,
        demoted: true,
      },
    });
  });

  // A runtime that changed the layout must read as unreadable, not as a
  // plausible but wrong person.
  test("refuses bytes that are not this layout", () => {
    const good = consumer({ lite: "a.01" });
    expect(() => decodeConsumer(good.slice(0, 70))).toThrow();
    expect(() => decodeConsumer(Uint8Array.from([...good, 0]))).toThrow(
      "unread",
    );
    const badCredibility = Uint8Array.from(good);
    badCredibility[65 + 1 + 5] = 7;
    expect(() => decodeConsumer(badCredibility)).toThrow("credibility");
  });
});

/** A connection that answers with scripted events once the follow is requested. */
function connection(script: (sent: Record<string, unknown>) => object[]) {
  const sent: Record<string, unknown>[] = [];
  const queue: object[] = [];
  let wake: (() => void) | null = null;
  let closed = false;
  const push = (messages: object[]) => {
    queue.push(...messages);
    wake?.();
  };
  const fake: JsonRpcConnection = {
    send(request) {
      const message = JSON.parse(request) as Record<string, unknown>;
      sent.push(message);
      push(script(message));
    },
    async *responses() {
      while (!closed) {
        const next = queue.shift();
        if (next === undefined) {
          await new Promise<void>((resolve) => (wake = resolve));
          continue;
        }
        yield JSON.stringify(next);
      }
    },
    close() {
      closed = true;
      wake?.();
    },
  };
  return { fake, sent, isClosed: () => closed };
}

const BLOCK = `0x${"ab".repeat(32)}` as HexString;
const event = (result: object) => ({
  method: "chainHead_v1_followEvent",
  params: { subscription: "sub", result },
});

/** A chain that holds `values` by key, and answers a storage read in one batch. */
function chainWith(values: Record<string, string>) {
  return connection((message) => {
    if (message.method === "chainHead_v1_follow")
      return [
        { id: 1, result: "sub" },
        event({ event: "initialized", finalizedBlockHashes: [BLOCK] }),
      ];
    if (message.method === "chainHead_v1_storage") {
      const items = (message.params as [string, string, { key: string }[]])[2]
        .filter(({ key }) => key in values)
        .map(({ key }) => ({ key, value: values[key] }));
      return [
        { id: message.id, result: { result: "started", operationId: "op" } },
        event({ event: "operationStorageItems", operationId: "op", items }),
        event({ event: "operationStorageDone", operationId: "op" }),
      ];
    }
    return [];
  });
}

describe("reading storage", () => {
  test("follows, reads at the finalized block, and closes", async () => {
    const key = mapKey(CONSUMERS, ACCOUNT);
    const { fake, sent, isClosed } = chainWith({ [key]: "0x0102" });
    const read = await readStorageValues(fake, [key, mapKey(CONSUMERS, OTHER)]);
    expect(read).toEqual({ blockHash: BLOCK, values: ["0x0102", null] });
    expect(sent.map((message) => message.method)).toEqual([
      "chainHead_v1_follow",
      "chainHead_v1_storage",
      "chainHead_v1_unfollow",
    ]);
    expect(sent[1].params).toEqual([
      "sub",
      BLOCK,
      [key, mapKey(CONSUMERS, OTHER)].map((k) => ({ key: k, type: "value" })),
      null,
    ]);
    expect(isClosed()).toBe(true);
  });

  test("fails, and closes, when the chain stops following", async () => {
    const { fake, isClosed } = connection((message) =>
      message.method === "chainHead_v1_follow"
        ? [{ id: 1, result: "sub" }, event({ event: "stop" })]
        : [],
    );
    await expect(readStorageValues(fake, ["0x00"])).rejects.toThrow("stopped");
    expect(isClosed()).toBe(true);
  });

  // A light client that never answers must not leave the menu loading.
  test("gives up after the timeout and closes", async () => {
    const { fake, isClosed } = connection(() => []);
    await expect(readStorageValues(fake, ["0x00"], 30)).rejects.toThrow(
      "no answer",
    );
    expect(isClosed()).toBe(true);
  });
});

describe("reading an account", () => {
  const accounts = [
    { role: "identity" as const, accountId: ACCOUNT_HEX },
    { role: "root" as const, accountId: OTHER_HEX },
  ];

  // The two accounts are separate questions: a record under one says nothing
  // about the other.
  test("reports each account by what the chain holds for it", async () => {
    const { fake } = chainWith({
      [mapKey(CONSUMERS, ACCOUNT)]: bytesToHex(consumer({ lite: "alice.07" })),
    });
    const reading = await readPeopleChain(
      { connect: () => Promise.resolve(fake) },
      `0x${"11".repeat(32)}`,
      accounts,
    );
    expect(reading.readings).toEqual([
      {
        role: "identity",
        accountId: ACCOUNT_HEX,
        litePerson: false,
        rings: NO_RINGS,
        consumer: {
          fullUsername: null,
          liteUsername: "alice.07",
          credibility: { tag: "Lite" },
        },
      },
      {
        role: "root",
        accountId: OTHER_HEX,
        litePerson: false,
        rings: NO_RINGS,
        consumer: null,
      },
    ]);
  });

  // An unreadable record is unknown, not absent.
  test("marks a record it cannot decode instead of calling it absent", async () => {
    const { fake } = chainWith({ [mapKey(CONSUMERS, ACCOUNT)]: "0x00" });
    const { readings } = await readPeopleChain(
      { connect: () => Promise.resolve(fake) },
      `0x${"11".repeat(32)}`,
      accounts,
    );
    expect(readings[0].consumer).toBeNull();
    expect(readings[0].problem).toContain("not the layout");
    expect(readings[1].problem).toBeUndefined();
  });
});

const LITE_PEOPLE =
  "276fc15f94f88f19ef554a8ff4374855e2491c72cc063f0098c08930123488ce";
const TO_PERSONAL_ID =
  "43b4b15ea5f08d8e3af42ba6b1f2e2b848a12cdd285062997937704d6c41f7d4";
const PEOPLE_RECORD =
  "43b4b15ea5f08d8e3af42ba6b1f2e2b843b4b15ea5f08d8e3af42ba6b1f2e2b8";
const MEMBERS =
  "ba7fb8745735dc3be2a2c61a72c39e78ba7fb8745735dc3be2a2c61a72c39e78";
const LITE_COLLECTION = bytesToHex(
  new TextEncoder().encode("pop:polkadot.network/people-lite"),
).slice(2);
const FULL_COLLECTION = bytesToHex(
  new TextEncoder().encode("pop:polkadot.network/people     "),
).slice(2);
const LITE_RING_KEY = new Uint8Array(32).fill(7);
const FULL_RING_KEY = new Uint8Array(32).fill(8);

describe("ring records", () => {
  test("decodes each position the pallet can store", () => {
    expect(decodeRingPosition(Uint8Array.from([2]))).toEqual({
      tag: "Suspended",
    });
    expect(
      decodeRingPosition(
        Uint8Array.from([1, 3, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0]),
      ),
    ).toEqual({ tag: "Included", ringIndex: 3, ringPage: 0, ringPosition: 12 });
    expect(
      decodeRingPosition(Uint8Array.from([0, 2, 0, 0, 0, ...u64(500)])),
    ).toEqual({ tag: "Onboarding", queuePage: 2, queuedAt: 500n });
  });

  test("refuses an unknown variant and trailing bytes", () => {
    expect(() => decodeRingPosition(Uint8Array.from([3]))).toThrow("variant");
    expect(() => decodeRingPosition(Uint8Array.from([2, 0]))).toThrow("unread");
  });

  test("reads the ring key out of a Lite and a full person record", () => {
    expect(
      decodeLitePersonKey(
        Uint8Array.from([...LITE_RING_KEY, 0, ...new Uint8Array(32)]),
      ),
    ).toBe(bytesToHex(LITE_RING_KEY));
    expect(() =>
      decodeLitePersonKey(Uint8Array.from([...LITE_RING_KEY, 1])),
    ).toThrow("method");
    expect(decodePersonKey(Uint8Array.from([...FULL_RING_KEY, 0]))).toBe(
      bytesToHex(FULL_RING_KEY),
    );
    expect(
      decodePersonKey(
        Uint8Array.from([...FULL_RING_KEY, 1, ...new Uint8Array(32)]),
      ),
    ).toBe(bytesToHex(FULL_RING_KEY));
  });
});

describe("reading ring membership", () => {
  const read = async (values: Record<string, string>) => {
    const { fake, sent } = chainWith(values);
    const reading = await readPeopleChain(
      { connect: () => Promise.resolve(fake) },
      `0x${"11".repeat(32)}`,
      [{ role: "identity", accountId: ACCOUNT_HEX }],
    );
    return { reading, sent };
  };
  const personalId = u64(42);

  // The full person's key is three dependent reads away. They must all be made
  // on the subscription and block of the first, or the key could come from an
  // older era than the membership it is looked up in.
  test("follows account, personal id and record to the Members entry on one block", async () => {
    const { reading, sent } = await read({
      [mapKey(LITE_PEOPLE, ACCOUNT)]: bytesToHex(
        Uint8Array.from([...LITE_RING_KEY, 0, ...new Uint8Array(32)]),
      ),
      [mapKey(`${MEMBERS}${LITE_COLLECTION}`, LITE_RING_KEY)]:
        "0x010300000000000000" + "0c000000",
      [mapKey(TO_PERSONAL_ID, ACCOUNT)]: bytesToHex(
        Uint8Array.from(personalId),
      ),
      [mapKey(PEOPLE_RECORD, Uint8Array.from(personalId))]: bytesToHex(
        Uint8Array.from([...FULL_RING_KEY, 0]),
      ),
      [mapKey(`${MEMBERS}${FULL_COLLECTION}`, FULL_RING_KEY)]: "0x02",
    });
    expect(reading.readings[0].rings).toEqual([
      {
        collection: "LitePeople",
        lookup: {
          tag: "Listed",
          key: bytesToHex(LITE_RING_KEY),
          position: {
            tag: "Included",
            ringIndex: 3,
            ringPage: 0,
            ringPosition: 12,
          },
        },
      },
      {
        collection: "People",
        lookup: {
          tag: "Listed",
          key: bytesToHex(FULL_RING_KEY),
          position: { tag: "Suspended" },
        },
      },
    ]);
    const storage = sent.filter((m) => m.method === "chainHead_v1_storage");
    expect(storage).toHaveLength(3);
    expect(storage.map((m) => (m.params as [string, string])[1])).toEqual([
      BLOCK,
      BLOCK,
      BLOCK,
    ]);
    expect(storage.map((m) => (m.params as [string])[0])).toEqual([
      "sub",
      "sub",
      "sub",
    ]);
  });

  // No named key means no lookup; a named key with no entry is its own fact.
  test("tells no key from a key the Members pallet does not list", async () => {
    const { reading, sent } = await read({
      [mapKey(LITE_PEOPLE, ACCOUNT)]: bytesToHex(
        Uint8Array.from([...LITE_RING_KEY, 0, ...new Uint8Array(32)]),
      ),
    });
    expect(reading.readings[0].rings).toEqual([
      {
        collection: "LitePeople",
        lookup: { tag: "NotListed", key: bytesToHex(LITE_RING_KEY) },
      },
      { collection: "People", lookup: { tag: "NoKey" } },
    ]);
    expect(
      sent.filter((m) => m.method === "chainHead_v1_storage"),
    ).toHaveLength(2);
  });

  test("marks a record that is not the layout unreadable, not absent", async () => {
    const { reading } = await read({
      [mapKey(LITE_PEOPLE, ACCOUNT)]: "0x0102",
      [mapKey(TO_PERSONAL_ID, ACCOUNT)]: bytesToHex(
        Uint8Array.from(personalId),
      ),
      [mapKey(PEOPLE_RECORD, Uint8Array.from(personalId))]: "0x00",
    });
    const [lite, full] = reading.readings[0].rings;
    expect(lite.lookup).toMatchObject({ tag: "Unreadable" });
    expect(full.lookup).toMatchObject({ tag: "Unreadable" });
  });
});
