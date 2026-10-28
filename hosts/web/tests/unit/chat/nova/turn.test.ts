import { describe, expect, test } from "bun:test";
import { blake2b } from "@noble/hashes/blake2.js";
import {
  PERSONHOOD_COLLECTIONS,
  ProofRefused,
  TURN_ISSUE_PATH,
  TURN_RETRY_AFTER_MS,
  TurnCredentials,
  proveForTurn,
  turnIdentity,
  turnMessage,
  type FetchLike,
  type ProofSource,
  type RingKey,
} from "../../../../src/chat/nova/turn.js";
import { relayNote } from "../../../../src/chat/nova-provider.js";

const GENESIS = `0x${"ab".repeat(32)}` as const;
const ring = (collection: `0x${string}`): RingKey["rings"][number] => ({
  chainId: GENESIS,
  junctions: [
    { tag: "PalletInstance", value: 61 },
    { tag: "CollectionId", value: collection },
  ],
});
/** The two keys the core registers for `peopl.<tld>`: full at index 0, Lite at index 1. */
const KEYS: RingKey[] = [
  {
    handle: {
      dotNsIdentifier: "peopl.paseo",
      derivationIndex: { tag: "Index", value: 0 },
    },
    rings: [ring(PERSONHOOD_COLLECTIONS.full)],
  },
  {
    handle: {
      dotNsIdentifier: "peopl.paseo",
      derivationIndex: { tag: "Index", value: 1 },
    },
    rings: [ring(PERSONHOOD_COLLECTIONS.lite)],
  },
];
const PROOF = `0x${"5e".repeat(16)}` as const;

/** A core stand-in: records what it is asked, and refuses with `refuse` when set. */
function core(refuse: Partial<Record<"list" | "lite" | "full", string>> = {}) {
  const asked: unknown[] = [];
  const source: ProofSource = {
    listRingVrfKeys: async (owner) => {
      asked.push({ list: owner });
      if (refuse.list) throw new ProofRefused(refuse.list);
      return KEYS;
    },
    createAccountProof: async (request) => {
      asked.push(request);
      const lite =
        request.ringLocation.junctions[1].value === PERSONHOOD_COLLECTIONS.lite;
      const tag = lite ? refuse.lite : refuse.full;
      if (tag) throw new ProofRefused(tag);
      return { proof: PROOF, ringIndex: 3, ringRevision: 7 };
    },
  };
  return { source, asked, disposed: { count: 0 } };
}

/** A backend stand-in that answers `answer` and records every request. */
function backend(answer: { status: number; body: unknown }) {
  const requests: { url: string; body: unknown }[] = [];
  const fetch: FetchLike = async (url, init) => {
    requests.push({ url, body: JSON.parse(init.body) });
    return {
      status: answer.status,
      json: async () => answer.body,
      text: async () => JSON.stringify(answer.body),
    };
  };
  return { fetch, requests };
}

const ISSUED = {
  servers: [
    "stun:stun.example.invalid:3478",
    "turn:turn.example.invalid:3478?transport=udp",
  ],
  username: "synthetic-user",
  password: "synthetic-secret",
  ttl: 1800,
};

function credentials(
  coreStub: ReturnType<typeof core>,
  service: ReturnType<typeof backend>,
  clock: { now: number },
  issuer: string | null = "https://issuer.invalid",
) {
  return new TurnCredentials({
    identity: turnIdentity("paseo"),
    issuer,
    proofs: async () => ({
      source: coreStub.source,
      dispose: () => void (coreStub.disposed.count += 1),
    }),
    fetch: service.fetch,
    now: () => clock.now,
  });
}

describe("TURN proof", () => {
  test("the proved message is BLAKE2b-256 of DUB's label and the timestamp as big-endian u64", () => {
    const stamp = new Uint8Array(8);
    new DataView(stamp.buffer).setBigUint64(0, 1_760_000_000n, false);
    const expected = blake2b(
      new Uint8Array([
        ...new TextEncoder().encode("dub/turn-credential/v1"),
        ...stamp,
      ]),
      { dkLen: 32 },
    );
    expect(turnMessage(1_760_000_000)).toEqual(expected);
    expect(PERSONHOOD_COLLECTIONS.lite).toBe(
      "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465",
    );
  });

  test("chat asks for Humanity's key, in the game's context, Lite ring first", async () => {
    const stub = core();
    const body = await proveForTurn(
      stub.source,
      turnIdentity("paseo"),
      1_760_000_000,
    );
    expect(stub.asked).toEqual([
      { list: "peopl.paseo" },
      {
        keyHandle: KEYS[1].handle,
        context: {
          productId: "dim2.paseo",
          suffix: { tag: "Index", value: 0 },
        },
        ringLocation: KEYS[1].rings[0],
        message: `0x${Buffer.from(turnMessage(1_760_000_000)).toString("hex")}`,
      },
    ]);
    expect(body).toEqual({
      productId: "dim2.paseo",
      collection: PERSONHOOD_COLLECTIONS.lite,
      proof: PROOF,
      ringIndex: 3,
      ringRevision: 7,
      timestamp: 1_760_000_000,
    });
    expect(turnIdentity("testnet")).toEqual({
      caller: "chat.testnet",
      keyOwner: "peopl.testnet",
      context: "dim2.testnet",
    });
  });

  test("a person only in the full ring proves there", async () => {
    const stub = core({ lite: "NotMember" });
    const body = await proveForTurn(stub.source, turnIdentity("paseo"), 1);
    expect(body).toMatchObject({ collection: PERSONHOOD_COLLECTIONS.full });
  });
});

describe("TURN credentials", () => {
  test("a missing grant asks the backend nothing, and is not asked again for a while", async () => {
    const stub = core({ lite: "NotAllowlisted" });
    const service = backend({ status: 201, body: ISSUED });
    const clock = { now: 1_000_000 };
    const turn = credentials(stub, service, clock);
    expect(await turn.iceServers()).toEqual([]);
    expect(service.requests).toEqual([]);
    const status = turn.status();
    expect(status).toMatchObject({
      state: "unavailable",
      problem: { code: "grant-missing" },
    });
    expect(relayNote(status)).toContain(
      'peopl.paseo and dim2.paseo must both grant chat.paseo the "context" scope',
    );
    // A second call in the wait asks the core nothing either.
    const before = stub.asked.length;
    expect(await turn.iceServers()).toEqual([]);
    expect(stub.asked.length).toBe(before);
    clock.now += TURN_RETRY_AFTER_MS["grant-missing"] + 1;
    await turn.iceServers();
    expect(stub.asked.length).toBeGreaterThan(before);
    expect(stub.disposed.count).toBe(2);
  });

  test("a granted proof is posted to the issuer, and the credentials are kept until near expiry", async () => {
    const stub = core();
    const service = backend({ status: 201, body: ISSUED });
    const clock = { now: 1_760_000_000_500 };
    const turn = credentials(stub, service, clock);
    const [servers, again] = await Promise.all([
      turn.iceServers(),
      turn.iceServers(),
    ]);
    expect(servers).toEqual([
      { urls: "stun:stun.example.invalid:3478" },
      {
        urls: "turn:turn.example.invalid:3478?transport=udp",
        username: "synthetic-user",
        credential: "synthetic-secret",
      },
    ]);
    expect(again).toBe(servers);
    expect(service.requests).toEqual([
      {
        url: `https://issuer.invalid${TURN_ISSUE_PATH}`,
        body: {
          productId: "dim2.paseo",
          collection: PERSONHOOD_COLLECTIONS.lite,
          proof: PROOF,
          ringIndex: 3,
          ringRevision: 7,
          timestamp: 1_760_000_000,
        },
      },
    ]);
    // What the screen sees holds no credential.
    const status = turn.status();
    expect(status).toEqual({
      state: "ready",
      expiresAt: expect.any(Number),
      servers: 2,
    });
    expect(JSON.stringify(status)).not.toContain("synthetic-secret");
    expect(relayNote(status)).toBeNull();
    await turn.iceServers();
    expect(service.requests.length).toBe(1);
    // Past expiry, a fresh proof buys fresh credentials.
    clock.now += ISSUED.ttl * 1000;
    await turn.iceServers();
    expect(service.requests.length).toBe(2);
  });

  test("a refusal of the backend gives no relay, says why, and keeps no credential", async () => {
    const stub = core();
    const service = backend({ status: 403, body: { error: "proof rejected" } });
    const turn = credentials(stub, service, { now: 5_000_000 });
    expect(await turn.iceServers()).toEqual([]);
    expect(turn.status()).toMatchObject({
      state: "unavailable",
      problem: {
        code: "issuer-refused",
        status: 403,
        reason: "The identity backend refused the proof (403): proof rejected.",
      },
    });
  });

  test("a network with no identity backend asks neither the core nor anyone else", async () => {
    const stub = core();
    const service = backend({ status: 201, body: ISSUED });
    const turn = credentials(stub, service, { now: 1 }, null);
    expect(await turn.iceServers()).toEqual([]);
    expect(stub.asked).toEqual([]);
    expect(service.requests).toEqual([]);
    expect(turn.status()).toMatchObject({
      problem: { code: "not-configured" },
    });
  });

  test("a session reset drops the credentials, and an answer still in flight reaches no one", async () => {
    const stub = core();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const service = backend({ status: 201, body: ISSUED });
    const slow: FetchLike = async (url, init) => {
      await gate;
      return service.fetch(url, init);
    };
    const turn = new TurnCredentials({
      identity: turnIdentity("paseo"),
      issuer: "https://issuer.invalid",
      proofs: async () => ({ source: stub.source, dispose: () => {} }),
      fetch: slow,
      now: () => 1_000_000,
    });
    const pending = turn.iceServers();
    turn.reset();
    release();
    expect(await pending).toEqual([]);
    expect(turn.status()).toEqual({ state: "idle" });
  });
});
