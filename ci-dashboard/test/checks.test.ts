import { describe, expect, test } from "bun:test";

import { checkLink, checkRpc } from "../src/checks";

const endpoint = {
  name: "People",
  url: "https://rpc.test/people",
  network: "Test",
  minPeers: 1,
};

function rpcFetch(results: Record<string, unknown>): typeof fetch {
  return (async (_url: string, init: RequestInit) => {
    const { method } = JSON.parse(String(init.body)) as { method: string };
    return Response.json({ jsonrpc: "2.0", id: 1, result: results[method] });
  }) as typeof fetch;
}

describe("checkRpc", () => {
  test("passes with enough peers and reads the block number", async () => {
    const result = await checkRpc(
      endpoint,
      rpcFetch({
        system_health: { peers: 4, isSyncing: false },
        chain_getHeader: { number: "0x20f0e4" },
      }),
    );
    expect(result).toMatchObject({ ok: true, block: 2_158_820, peers: 4 });
  });

  test("fails below the peer minimum", async () => {
    const result = await checkRpc(
      endpoint,
      rpcFetch({
        system_health: { peers: 0, isSyncing: false },
        chain_getHeader: { number: "0x1" },
      }),
    );
    expect(result).toMatchObject({ ok: false, detail: "0 peers, needs 1" });
  });

  test("allows no peers where the network has none", async () => {
    const result = await checkRpc(
      { ...endpoint, minPeers: 0 },
      rpcFetch({
        system_health: { peers: 0, isSyncing: false },
        chain_getHeader: { number: "0x1" },
      }),
    );
    expect(result.ok).toBe(true);
  });

  test("fails while syncing", async () => {
    const result = await checkRpc(
      endpoint,
      rpcFetch({
        system_health: { peers: 5, isSyncing: true },
        chain_getHeader: { number: "0x1" },
      }),
    );
    expect(result).toMatchObject({ ok: false, detail: "syncing" });
  });

  test("reports an HTTP error", async () => {
    const result = await checkRpc(
      endpoint,
      (async () =>
        new Response("", { status: 404 })) as unknown as typeof fetch,
    );
    expect(result).toMatchObject({ ok: false, detail: "HTTP 404" });
  });
});

describe("checkLink", () => {
  const link = {
    name: "Docs",
    url: "https://docs.test/",
    method: "GET" as const,
    lookFor: "TrUAPI Explorer",
  };

  test("passes when the page has the expected text", async () => {
    const result = await checkLink(
      link,
      (async () =>
        new Response(
          "<title>TrUAPI Explorer</title>",
        )) as unknown as typeof fetch,
    );
    expect(result).toMatchObject({ ok: true, detail: "HTTP 200" });
  });

  test("fails when the text is missing", async () => {
    const result = await checkLink(
      link,
      (async () =>
        new Response(
          "<title>Site not found</title>",
        )) as unknown as typeof fetch,
    );
    expect(result).toMatchObject({
      ok: false,
      detail: '"TrUAPI Explorer" not found',
    });
  });

  test("fails on an error status", async () => {
    const result = await checkLink(
      link,
      (async () =>
        new Response("", { status: 503 })) as unknown as typeof fetch,
    );
    expect(result).toMatchObject({ ok: false, detail: "HTTP 503" });
  });
});
