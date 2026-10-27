import { describe, expect, test } from "bun:test";
import { type DotnsTransport, DOTNS_KEYS, resolveDotns } from "./dotns.js";
import { type Descriptor, memoryStore, parse } from "./index.js";

const POINTER = { url: "https://stack.example/.well-known/trinity-stack.json" };
const DISPATCHER = `0x${"d1".repeat(20)}`;
const ROOT_DISPATCHER = `0x${"d0".repeat(20)}`;
const REGISTRY = `0x${"ee".repeat(20)}`;
const REGISTRAR = `0x${"aa".repeat(20)}`;
const RESOLVER = `0x${"bb".repeat(20)}`;

const word = (address: string) => `0x${address.slice(2).padStart(64, "0")}`;
const key = (name: string) =>
  `0x8eaa6ac0${Buffer.from(name).toString("hex").padEnd(64, "0")}`;

function descriptor(generation: string): Descriptor {
  const document = {
    $v: 1,
    sequence: 1,
    generation,
    id: "local",
    name: "Local",
    network: "local",
    tld: "local",
    services: [],
    chains: [
      {
        role: "Relay",
        genesisHash: `0x${"86".repeat(32)}`,
        rpc: ["ws://127.0.0.1:9944"],
      },
    ],
  };
  return parse(new TextEncoder().encode(JSON.stringify(document))).descriptor;
}

/** A chain whose gateway dispatches to `dispatcher`, counting the calls made to it. */
function chain(dispatcher: string | null) {
  const calls = { count: 0 };
  const answers = new Map([
    [`${DISPATCHER}:0x7656419f`, word(REGISTRY)],
    [`${ROOT_DISPATCHER}:0xcc1f2afa`, word(DISPATCHER)],
    [`${REGISTRY}:${key("registrar")}`, word(REGISTRAR)],
    [`${REGISTRY}:${key("resolver")}`, word(RESOLVER)],
  ]);
  const transport: DotnsTransport = {
    storage: async () => {
      calls.count++;
      return dispatcher;
    },
    call: async (to, data) => {
      calls.count++;
      if (to === REGISTRY && data.startsWith("0x8eaa6ac0"))
        return answers.get(`${to}:${data}`) ?? word(`0x${"0".repeat(40)}`);
      const answer = answers.get(`${to}:${data}`);
      if (!answer) throw new Error("execution reverted");
      return answer;
    },
  };
  return { transport, calls };
}

const resolved = (
  generation: string,
  resolvedAt: number,
  dispatcher = DISPATCHER,
) => ({
  dispatcher,
  protocolRegistry: REGISTRY,
  addresses: { registrar: REGISTRAR, resolver: RESOLVER },
  generation,
  resolvedAt,
});

const at = (seconds: number) => new Date(seconds * 1000);

describe("resolveDotns", () => {
  test("follows the gateway's dispatcher to the registry and leaves unset keys out", async () => {
    const { transport } = chain(DISPATCHER);
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        transport,
        now: at(100),
      }),
    ).toEqual(resolved("g1", 100));
  });

  test("reaches the registry through a dispatcher that forwards to TARGET()", async () => {
    const { transport } = chain(ROOT_DISPATCHER);
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        transport,
        now: at(100),
      }),
    ).toEqual(resolved("g1", 100, ROOT_DISPATCHER));
  });

  test("is undefined when the gateway has no dispatcher", async () => {
    const { transport } = chain(null);
    expect(
      await resolveDotns(POINTER, descriptor("g1"), { transport }),
    ).toBeUndefined();
  });

  test("asks every registry key", async () => {
    const { transport, calls } = chain(DISPATCHER);
    await resolveDotns(POINTER, descriptor("g1"), { transport });
    // The storage read, protocolRegistry(), then one get per key.
    expect(calls.count).toBe(2 + DOTNS_KEYS.length);
  });

  test("a stored resolution is reused until it ages out or the chains restart", async () => {
    const store = memoryStore();
    const { transport, calls } = chain(DISPATCHER);
    const options = { store, transport, maxAgeMs: 60_000 };
    await resolveDotns(POINTER, descriptor("g1"), { ...options, now: at(100) });
    const afterFirst = calls.count;
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        ...options,
        now: at(159),
      }),
    ).toEqual(resolved("g1", 100));
    expect(calls.count).toBe(afterFirst);
    await resolveDotns(POINTER, descriptor("g1"), { ...options, now: at(160) });
    expect(calls.count).toBe(2 * afterFirst);
    expect(
      await resolveDotns(POINTER, descriptor("g2"), {
        ...options,
        now: at(161),
      }),
    ).toEqual(resolved("g2", 161));
  });

  test("a failed refresh falls back to the stored resolution of the same generation only", async () => {
    const store = memoryStore();
    const { transport } = chain(DISPATCHER);
    await resolveDotns(POINTER, descriptor("g1"), {
      store,
      transport,
      now: at(100),
    });
    const down: DotnsTransport = {
      storage: () => Promise.reject(new Error("unreachable")),
      call: () => Promise.reject(new Error("unreachable")),
    };
    const later = { store, transport: down, maxAgeMs: 0, now: at(200) };
    expect(await resolveDotns(POINTER, descriptor("g1"), later)).toEqual(
      resolved("g1", 100),
    );
    await expect(
      resolveDotns(POINTER, descriptor("g2"), later),
    ).rejects.toThrow("unreachable");
  });

  test("a stack that names no eth-rpc needs a transport", async () => {
    await expect(resolveDotns(POINTER, descriptor("g1"))).rejects.toThrow(
      "pass a transport",
    );
  });
});
