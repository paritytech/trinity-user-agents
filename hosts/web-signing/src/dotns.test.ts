import { describe, expect, test } from "bun:test";
import { blake2b } from "@noble/hashes/blake2.js";
import { keccak_256 } from "@noble/hashes/sha3.js";
import {
  bytesToHex,
  chooseSource,
  classifyContent,
  contenthashToCid,
  gatewayUrl,
  hexToBytes,
  httpRpc,
  looksLikeCar,
  namehash,
  PASEO_DOTNS,
  resolveContentCid,
  type ContentKind,
  type RpcCall,
} from "./dotns.js";

/** The CID `dotkit asset-hub name resolve chat-spa-probe` printed. */
const PROBE_CID = "bafybeihmnm4dydovrq5jle7qgnyswajzdlu3cuncws32p5phwithaomsbe";

/** Decode the multibase-b CID back to bytes, to build the contenthash record. */
function cidBytes(cid: string): Uint8Array {
  const alphabet = "abcdefghijklmnopqrstuvwxyz234567";
  let bits = 0;
  let buffer = 0;
  const out: number[] = [];
  for (const char of cid.slice(1)) {
    buffer = (buffer << 5) | alphabet.indexOf(char);
    bits += 5;
    if (bits >= 8) {
      out.push((buffer >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Uint8Array.from(out);
}

const record = (cid: string) => Uint8Array.from([0xe3, 0x01, ...cidBytes(cid)]);

describe("namehash", () => {
  // The standard ENS vectors: the algorithm is ENS's, and a wrong node reads
  // as "name not found" on chain.
  test("matches the ENS reference vectors", () => {
    expect(bytesToHex(namehash(""))).toBe("0".repeat(64));
    expect(bytesToHex(namehash("eth"))).toBe(
      "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae",
    );
    expect(bytesToHex(namehash("foo.eth"))).toBe(
      "de9b09fd7c5f901e23a3f19fecc54828e9c848539801e86591bd9801b019f84f",
    );
  });
});

describe("contenthashToCid", () => {
  test("turns an IPFS contenthash into the CID dotkit reports", () => {
    expect(contenthashToCid(record(PROBE_CID))).toBe(PROBE_CID);
  });

  test("refuses other contenthash kinds and malformed records", () => {
    expect(contenthashToCid(Uint8Array.of(0xe4, 0x01, 0x01, 0x70))).toBeNull();
    expect(contenthashToCid(Uint8Array.of(0xe3, 0x01))).toBeNull();
    // CIDv0 or garbage after the tag is not a CIDv1.
    expect(
      contenthashToCid(Uint8Array.of(0xe3, 0x01, 0x12, 0x20, 1)),
    ).toBeNull();
    expect(contenthashToCid(new Uint8Array(70).fill(1))).toBeNull();
  });

  // The CID becomes part of a URL, so it must only ever hold base32 characters.
  test("only produces base32 text", () => {
    const hostile = Uint8Array.from([
      0xe3,
      0x01,
      0x01,
      ...new Uint8Array(40).fill(0xff),
    ]);
    expect(contenthashToCid(hostile)).toMatch(/^b[a-z2-7]+$/);
  });
});

describe("gatewayUrl", () => {
  test("builds the gateway path form and keeps the typed suffix", () => {
    expect(gatewayUrl(PROBE_CID, "").href).toBe(
      `${PASEO_DOTNS.contentGateway}/ipfs/${PROBE_CID}/`,
    );
    expect(gatewayUrl(PROBE_CID, "a/b?x=1#top").href).toBe(
      `${PASEO_DOTNS.contentGateway}/ipfs/${PROBE_CID}/a/b?x=1#top`,
    );
  });

  test("refuses text that is not a base32 CID", () => {
    expect(() => gatewayUrl("../evil", "")).toThrow();
    expect(() => gatewayUrl(`${PROBE_CID}/../..`, "")).toThrow();
  });

  test("never leaves the gateway origin, whatever the suffix says", () => {
    expect(gatewayUrl(PROBE_CID, "//evil.test/x").origin).toBe(
      PASEO_DOTNS.contentGateway,
    );
  });
});

/**
 * A stand-in for Asset Hub that stores `contenthash` the way the contract
 * does: the resolver's child trie id behind `Revive::AccountInfoOf`, and one
 * Solidity `bytes` value in that child trie, inline when short and spread over
 * words from `keccak256(slotKey)` when long.
 */
function fakeChain(name: string, contenthash: Uint8Array | null): RpcCall {
  const trieId = Uint8Array.from({ length: 32 }, (_, i) => i + 1);
  const store = new Map<string, Uint8Array>();
  const slotKey = (() => {
    const word = new Uint8Array(32);
    return keccak_256(Uint8Array.from([...namehash(name), ...word]));
  })();
  const put = (key: Uint8Array, value: Uint8Array) =>
    store.set(bytesToHex(blake2b(key, { dkLen: 32 })), value);

  if (contenthash !== null) {
    if (contenthash.length < 32) {
      const slot = new Uint8Array(32);
      slot.set(contenthash);
      slot[31] = contenthash.length * 2;
      put(slotKey, slot);
    } else {
      const slot = new Uint8Array(32);
      new DataView(slot.buffer).setUint32(28, contenthash.length * 2 + 1);
      put(slotKey, slot);
      let base = keccak_256(slotKey);
      for (let at = 0; at < contenthash.length; at += 32) {
        const word = new Uint8Array(32);
        word.set(contenthash.subarray(at, at + 32));
        put(base, word);
        for (let i = 31; i >= 0; i -= 1) {
          if (base[i] === 0xff) base[i] = 0;
          else {
            base[i] += 1;
            break;
          }
        }
      }
    }
  }
  const accountKey =
    "0x735f040a5d490f1107ad9c56f5ca00d2ae37ff0591fdbbcd9c2406df7147a9dc" +
    PASEO_DOTNS.contentResolver;
  return async (method, params) => {
    if (method === "state_getStorage")
      return params[0] === accountKey
        ? `0x00${bytesToHex(Uint8Array.of(trieId.length << 2))}${bytesToHex(trieId)}ff`
        : null;
    if (method === "childstate_getStorage") {
      const prefix = `0x${bytesToHex(
        new TextEncoder().encode(":child_storage:default:"),
      )}${bytesToHex(trieId)}`;
      if (params[0] !== prefix) return null;
      const value = store.get(params[1].slice(2));
      return value ? `0x${bytesToHex(value)}` : null;
    }
    throw new Error(`unexpected ${method}`);
  };
}

describe("resolveContentCid", () => {
  test("reads a long contenthash from the resolver's child trie", async () => {
    const rpc = fakeChain("chat-spa-probe.paseo", record(PROBE_CID));
    expect(await resolveContentCid("chat-spa-probe.paseo", rpc)).toBe(
      PROBE_CID,
    );
  });

  test("reads a short value stored inline in the slot", async () => {
    const short = Uint8Array.from([0xe3, 0x01, 0x01, 0x55, 0x00, 0xaa]);
    const rpc = fakeChain("tiny.paseo", short);
    expect(await resolveContentCid("tiny.paseo", rpc)).toBe(
      contenthashToCid(short),
    );
  });

  // The mapping is keyed by namehash, so another name must not read this
  // name's record.
  test("does not answer for a different name", async () => {
    const rpc = fakeChain("chat-spa-probe.paseo", record(PROBE_CID));
    expect(await resolveContentCid("other.paseo", rpc)).toBeNull();
  });

  test("returns null for a name with no contenthash", async () => {
    expect(
      await resolveContentCid("empty.paseo", fakeChain("empty.paseo", null)),
    ).toBeNull();
  });

  test("returns null when the record is not an IPFS contenthash", async () => {
    const rpc = fakeChain(
      "odd.paseo",
      Uint8Array.from([0xe4, 0x01, ...new Uint8Array(40).fill(1)]),
    );
    expect(await resolveContentCid("odd.paseo", rpc)).toBeNull();
  });

  test("returns null when the resolver contract is not on chain", async () => {
    const rpc: RpcCall = async () => null;
    expect(await resolveContentCid("chat-spa-probe.paseo", rpc)).toBeNull();
  });

  test("passes an RPC failure on instead of reporting a missing name", async () => {
    const rpc: RpcCall = async () => {
      throw new Error("lost the connection");
    };
    await expect(
      resolveContentCid("chat-spa-probe.paseo", rpc),
    ).rejects.toThrow("lost the connection");
  });
});

describe("hexToBytes", () => {
  test("refuses text that is not hexadecimal", () => {
    expect(() => hexToBytes("0xzz")).toThrow();
    expect(() => hexToBytes("0x123")).toThrow();
  });
});

/** The first bytes of a real product CAR: header length 0x3a, then `{roots, version}`. */
const CAR_START = Uint8Array.from([
  0x3a, 0xa2, 0x65, 0x72, 0x6f, 0x6f, 0x74, 0x73, 0x81, 0xd8, 0x2a, 0x58, 0x25,
  0x00, 0x01, 0x70,
]);

describe("looksLikeCar", () => {
  test("recognises a CARv1 header", () => {
    expect(looksLikeCar(CAR_START)).toBe(true);
  });

  test("accepts a header that lists version first", () => {
    const bytes = Uint8Array.from([
      0x3a,
      0xa2,
      0x67,
      ...new TextEncoder().encode("version"),
      0x01,
    ]);
    expect(looksLikeCar(bytes)).toBe(true);
  });

  test("refuses a page, an image and short input", () => {
    expect(looksLikeCar(new TextEncoder().encode("<!doctype html>"))).toBe(
      false,
    );
    expect(
      looksLikeCar(Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a])),
    ).toBe(false);
    expect(looksLikeCar(new Uint8Array(0))).toBe(false);
  });
});

/** A gateway response with a content type and a body. */
function respond(type: string, body: Uint8Array, status = 200): typeof fetch {
  return (async () =>
    new Response(body as BodyInit, {
      status,
      headers: { "content-type": type },
    })) as unknown as typeof fetch;
}

describe("classifyContent", () => {
  test("calls an HTML response a site", async () => {
    const fetchFn = respond("text/html", new TextEncoder().encode("<html>"));
    expect(await classifyContent(PROBE_CID, fetchFn)).toBe("site");
  });

  // The gateway serves an app executable's archive as an opaque file, which is
  // exactly what an iframe cannot render as a site.
  test("calls an octet-stream CAR archive a car", async () => {
    expect(
      await classifyContent(
        PROBE_CID,
        respond("application/octet-stream", CAR_START),
      ),
    ).toBe("car");
  });

  test("calls any other file other", async () => {
    expect(
      await classifyContent(
        PROBE_CID,
        respond("image/png", Uint8Array.of(0x89, 0x50)),
      ),
    ).toBe("other");
  });

  test("asks the gateway path for the CID and only for the first bytes", async () => {
    let seen: { url: string; range: string | null } | undefined;
    const fetchFn = (async (url: string, init?: RequestInit) => {
      seen = {
        url,
        range: new Headers(init?.headers).get("range"),
      };
      return new Response("<html>", {
        headers: { "content-type": "text/html" },
      });
    }) as unknown as typeof fetch;
    await classifyContent(PROBE_CID, fetchFn);
    expect(seen).toEqual({
      url: `${PASEO_DOTNS.contentGateway}/ipfs/${PROBE_CID}/`,
      range: "bytes=0-15",
    });
  });

  /** A body that arrives as the given chunks, and records whether it was cancelled. */
  function chunked(chunks: Uint8Array[]) {
    const state = { pulled: 0, cancelled: false };
    const body = new ReadableStream<Uint8Array>({
      pull(controller) {
        const chunk = chunks[state.pulled];
        state.pulled += 1;
        if (chunk === undefined) controller.close();
        else controller.enqueue(chunk);
      },
      cancel() {
        state.cancelled = true;
      },
    });
    const fetchFn = (async () =>
      new Response(body, {
        headers: { "content-type": "application/octet-stream" },
      })) as unknown as typeof fetch;
    return { state, fetchFn };
  }

  test("recognises a CAR whose header arrives one byte at a time", async () => {
    const { fetchFn } = chunked(
      Array.from(CAR_START, (byte) => Uint8Array.of(byte)),
    );
    expect(await classifyContent(PROBE_CID, fetchFn)).toBe("car");
  });

  test("recognises a CAR whose header is split mid-word", async () => {
    const { fetchFn } = chunked([CAR_START.slice(0, 4), CAR_START.slice(4)]);
    expect(await classifyContent(PROBE_CID, fetchFn)).toBe("car");
  });

  test("reads no more than the prefix from a gateway that ignores Range", async () => {
    const chunks = [
      CAR_START.slice(0, 5),
      CAR_START.slice(5),
      new Uint8Array(1024),
      new Uint8Array(1024),
    ];
    const { state, fetchFn } = chunked(chunks);
    expect(await classifyContent(PROBE_CID, fetchFn)).toBe("car");
    expect(state.cancelled).toBe(true);
    expect(state.pulled).toBeLessThan(chunks.length + 1);
  });

  test("calls a short fragmented body that is not a CAR other", async () => {
    const { fetchFn } = chunked([Uint8Array.of(0x89), Uint8Array.of(0x50)]);
    expect(await classifyContent(PROBE_CID, fetchFn)).toBe("other");
  });

  test("reports a gateway error instead of guessing", async () => {
    await expect(
      classifyContent(PROBE_CID, respond("text/plain", new Uint8Array(0), 502)),
    ).rejects.toThrow("502");
  });
});

/** Readers over a table of records to CIDs and CIDs to kinds. */
function readers(
  records: Record<string, string>,
  kinds: Record<string, ContentKind>,
) {
  const asked: string[] = [];
  return {
    asked,
    readCid: async (record: string) => {
      asked.push(record);
      return records[record] ?? null;
    },
    classify: async (cid: string) => kinds[cid] ?? "other",
  };
}

describe("chooseSource", () => {
  test("opens the app executable first, whether it is an archive or a site", async () => {
    for (const kind of ["car", "site"] as const) {
      const r = readers(
        { "app.myapp.paseo": "bappappappapp", "myapp.paseo": "bsitesitesite" },
        { bappappappapp: kind, bsitesitesite: "site" },
      );
      const choice = await chooseSource("myapp.paseo", r);
      expect(choice.source).toEqual({
        record: "app.myapp.paseo",
        cid: "bappappappapp",
        kind,
      });
      expect(r.asked).toEqual(["app.myapp.paseo"]);
      expect(choice.skipped).toEqual([]);
    }
  });

  // The website record can be a different build from the app executable, so
  // falling back to it has to be reported and not silent.
  test("falls back to the website and reports the record it skipped", async () => {
    const r = readers(
      { "app.myapp.paseo": "bappappappapp", "myapp.paseo": "bsitesitesite" },
      { bappappappapp: "other", bsitesitesite: "site" },
    );
    const choice = await chooseSource("myapp.paseo", r);
    expect(choice.source?.record).toBe("myapp.paseo");
    expect(choice.skipped).toEqual([
      { record: "app.myapp.paseo", cid: "bappappappapp", kind: "other" },
    ]);
  });

  test("falls back to the website when there is no app record", async () => {
    const r = readers(
      { "myapp.paseo": "bsitesitesite" },
      { bsitesitesite: "site" },
    );
    const choice = await chooseSource("myapp.paseo", r);
    expect(choice.source?.record).toBe("myapp.paseo");
    expect(choice.skipped).toEqual([]);
  });

  test("finds no source when the only record is neither a site nor an archive", async () => {
    const r = readers({ "app.myapp.paseo": "bappappappapp" }, {});
    expect(await chooseSource("myapp.paseo", r)).toEqual({
      source: null,
      skipped: [
        { record: "app.myapp.paseo", cid: "bappappappapp", kind: "other" },
      ],
    });
  });

  test("finds nothing for a name with no records", async () => {
    expect(await chooseSource("myapp.paseo", readers({}, {}))).toEqual({
      source: null,
      skipped: [],
    });
  });

  test("passes a reader failure on instead of reporting a missing name", async () => {
    const failing = {
      readCid: async () => {
        throw new Error("lost the connection");
      },
      classify: async () => "site" as const,
    };
    await expect(chooseSource("myapp.paseo", failing)).rejects.toThrow(
      "lost the connection",
    );
  });
});

describe("httpRpc", () => {
  const URL_WSS = "wss://rpc.test";
  const reply = (body: unknown, status = 200) =>
    new Response(JSON.stringify(body), { status });

  test("posts a JSON-RPC request to the https form of the endpoint and returns the result", async () => {
    const seen: { url: string; method?: string; body: unknown }[] = [];
    const fetchFn = (async (url: string, init?: RequestInit) => {
      seen.push({
        url,
        method: init?.method,
        body: JSON.parse(String(init?.body)),
      });
      return reply({ jsonrpc: "2.0", id: 1, result: "0xabcd" });
    }) as unknown as typeof fetch;
    const call = httpRpc(URL_WSS, fetchFn);
    expect(await call("state_getStorage", ["0x01"])).toBe("0xabcd");
    await call("state_getStorage", ["0x02"]);
    expect(seen).toEqual([
      {
        url: "https://rpc.test",
        method: "POST",
        body: {
          jsonrpc: "2.0",
          id: 1,
          method: "state_getStorage",
          params: ["0x01"],
        },
      },
      {
        url: "https://rpc.test",
        method: "POST",
        body: {
          jsonrpc: "2.0",
          id: 2,
          method: "state_getStorage",
          params: ["0x02"],
        },
      },
    ]);
  });

  test("returns null for a key with no value, as a missing record", async () => {
    const call = httpRpc(URL_WSS, (async () =>
      reply({ jsonrpc: "2.0", id: 1, result: null })) as never);
    expect(await call("state_getStorage", ["0x01"])).toBeNull();
  });

  test("reports a JSON-RPC error, an HTTP error and an unreachable endpoint apart", async () => {
    const rpcError = httpRpc(URL_WSS, (async () =>
      reply({ jsonrpc: "2.0", id: 1, error: { code: -32601 } })) as never);
    await expect(rpcError("nope", [])).rejects.toThrow("-32601");
    const http = httpRpc(URL_WSS, (async () => reply({}, 503)) as never);
    await expect(http("state_getStorage", [])).rejects.toThrow("answered 503");
    const offline = httpRpc(URL_WSS, (async () => {
      throw new TypeError("Failed to fetch");
    }) as never);
    await expect(offline("state_getStorage", [])).rejects.toThrow(
      "cannot reach https://rpc.test",
    );
  });

  test("names the call that got no answer in time", async () => {
    const hangs = ((_url: string, init?: RequestInit) =>
      new Promise((_resolve, reject) =>
        init?.signal?.addEventListener("abort", () =>
          reject(init.signal?.reason),
        ),
      )) as unknown as typeof fetch;
    await expect(
      httpRpc(URL_WSS, hangs, 20)("childstate_getStorage", []),
    ).rejects.toThrow(
      "no answer from https://rpc.test for childstate_getStorage",
    );
  });
});
