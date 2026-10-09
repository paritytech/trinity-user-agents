import { afterEach, describe, expect, it } from "bun:test";
import { BrowserReceivingTransport, createReceiverEnrollment } from "./browser-receiving-transport.js";
import type { ReceivingAuthority } from "./runtime.js";

const originalFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = originalFetch; });

const authority: ReceivingAuthority = {
  productId: "receiver.test", account: "11".repeat(32), environment: "test",
  artifact: "22".repeat(32), genesis: "33".repeat(32), generation: 4n,
  osPermission: true, transportReady: true,
};

const decodeHex = (value: string) => Uint8Array.from(value.match(/../g)!, byte => Number.parseInt(byte, 16));

describe("browser receiving relay v2 transport", () => {
  it("preserves the original core-consent expiry beyond 24 hours without extending it", async () => {
    const enrollment = await createReceiverEnrollment(authority);
    enrollment.relayRevision = 7;
    enrollment.coreRevision = 9007199254740993n;
    const expiresAt = Date.now() + 30 * 24 * 60 * 60 * 1000;
    enrollment.routes = { watch: "44".repeat(32) };
    let request: Request | undefined;
    globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
      request = new Request(input, init);
      return new Response(JSON.stringify({ ok: true }));
    }) as typeof fetch;
    const transport = new BrowserReceivingTransport("https://relay.test", "https://host.test");
    await transport.register(enrollment, {
      authority, revision: enrollment.coreRevision, enabled: true, syncPending: true,
      watches: [{ id: "watch", genesis: authority.genesis, channel: "55".repeat(32), topics: ["66".repeat(32)],
        senders: ["88".repeat(32), "77".repeat(32)], expiresAt: BigInt(expiresAt),
        mutedUntil: 18446744073709551615n, route: "private-product-route" }],
    }, { endpoint: "https://push.test/subscription", keys: { auth: "auth", p256dh: "p256dh" } });
    expect(request!.url).toBe(`https://relay.test/v2/receivers/${enrollment.deviceId}`);
    expect(request!.headers.get("authorization")).toBe(`Bearer ${enrollment.secret}`);
    const wire = await request!.json();
    expect(wire.revision).toBe(7);
    expect(wire.watches[0].expiresAt).toBe(expiresAt);
    expect(wire.watches[0].mutedUntil).toBe(Number.MAX_SAFE_INTEGER);
    expect(wire.watches[0].senderKeys).toEqual(["77".repeat(32), "88".repeat(32)]);
    expect(wire.watches[0].routeToken).toBe(enrollment.routes.watch);
    const serialized = JSON.stringify(wire);
    for (const secret of [authority.account, authority.artifact, "private-product-route", enrollment.coreRevision.toString()]) {
      expect(serialized.includes(secret)).toBe(false);
    }
    const publicKey = await crypto.subtle.importKey("raw", decodeHex(wire.binding.ownerKey), "Ed25519", false, ["verify"]);
    expect(await crypto.subtle.verify("Ed25519", publicKey, decodeHex(wire.binding.signature), new TextEncoder().encode(JSON.stringify([
      "truapi:receiver-binding:v2", 2, authority.productId, "https://host.test", enrollment.deviceId,
      enrollment.ownerKey, wire.binding.issuedAt,
    ])))).toBe(true);
  });

  it("decodes retained frames only for the exact transport revision", async () => {
    const enrollment = await createReceiverEnrollment(authority);
    enrollment.relayRevision = 8;
    let revision = 8;
    globalThis.fetch = (async () => Response.json({ v: 2, revision, watchId: "watch",
      genesis: authority.genesis, channel: "55".repeat(32), topics: ["66".repeat(32)], frame: "AQID" })) as typeof fetch;
    const transport = new BrowserReceivingTransport("https://relay.test", "https://host.test");
    expect((await transport.event(enrollment, "event")).frame).toEqual(new Uint8Array([1, 2, 3]));
    revision = 9;
    await expect(transport.event(enrollment, "event")).rejects.toThrow("invalid retained receiving event");
  });

  it("revokes with the transport revision and bearer, never core revision", async () => {
    const enrollment = await createReceiverEnrollment(authority);
    enrollment.relayRevision = 10;
    enrollment.coreRevision = 1000n;
    let request: Request | undefined;
    globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
      request = new Request(input, init);
      return new Response(null, { status: 204 });
    }) as typeof fetch;
    await new BrowserReceivingTransport("https://relay.test", "https://host.test").revoke(enrollment);
    expect(request!.method).toBe("DELETE");
    expect(await request!.json()).toEqual({ revision: 10 });
  });

  it("rejects nonlocal plaintext relays and oversized retained responses", async () => {
    expect(() => new BrowserReceivingTransport("http://relay.test", "https://host.test")).toThrow("requires HTTPS");
    const enrollment = await createReceiverEnrollment(authority);
    globalThis.fetch = (async () => new Response("x".repeat(384 * 1024 + 1))) as typeof fetch;
    await expect(new BrowserReceivingTransport("https://relay.test", "https://host.test").event(enrollment, "event"))
      .rejects.toThrow("oversized receiving relay response");
  });
});
