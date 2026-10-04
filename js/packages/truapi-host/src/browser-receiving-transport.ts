import type { ReceivingAuthority, ReceivingRegistration } from "./runtime.js";

const hex = (bytes: ArrayBuffer | Uint8Array): string =>
  Array.from(new Uint8Array(bytes instanceof Uint8Array ? bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) : bytes), byte => byte.toString(16).padStart(2, "0")).join("");
export const randomReceiverToken = (): string => hex(crypto.getRandomValues(new Uint8Array(32)));

/** Host-private transport credentials. Never expose these records to products. */
export interface BrowserReceiverEnrollment {
  authority: ReceivingAuthority;
  deviceId: string;
  secret: string;
  ownerKey: string;
  privateKey: CryptoKey;
  relayRevision: number;
  coreRevision: bigint;
  routes: Record<string, string>;
  acknowledged: boolean;
  revoked: boolean;
}

export async function createReceiverEnrollment(authority: ReceivingAuthority): Promise<BrowserReceiverEnrollment> {
  const keys = await crypto.subtle.generateKey({ name: "Ed25519" }, false, ["sign", "verify"]) as CryptoKeyPair;
  const secret = randomReceiverToken();
  const secretBytes = Uint8Array.from(secret.match(/../g)!, value => Number.parseInt(value, 16));
  return {
    authority, secret, privateKey: keys.privateKey,
    deviceId: hex(await crypto.subtle.digest("SHA-256", secretBytes)),
    ownerKey: hex(await crypto.subtle.exportKey("raw", keys.publicKey)),
    relayRevision: 0, coreRevision: 0n, routes: {},
    acknowledged: false, revoked: false,
  };
}

/** The configured relay is trusted transport, never authority for app content. */
export class BrowserReceivingTransport {
  private readonly base: URL;
  constructor(relayUrl: string, private readonly origin: string) {
    this.base = new URL(relayUrl);
    if (this.base.protocol !== "https:" && !(this.base.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(this.base.hostname))) {
      throw new Error("receiving relay requires HTTPS");
    }
    if (this.base.username || this.base.password || this.base.search || this.base.hash) throw new Error("invalid receiving relay URL");
  }

  private async request(path: string, enrollment: BrowserReceiverEnrollment, method: string, body?: unknown): Promise<unknown> {
    const controller = new AbortController();
    const deadline = setTimeout(() => controller.abort(), 10_000);
    try {
      const response = await fetch(new URL(`${this.base.pathname.replace(/\/$/, "")}${path}`, this.base.origin), {
        method, credentials: "omit", redirect: "error", cache: "no-store", signal: controller.signal,
        headers: { Authorization: `Bearer ${enrollment.secret}`, "Content-Type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (!response.ok) throw new Error(`receiving relay HTTP ${response.status}`);
      if (method !== "GET") { await response.body?.cancel(); return undefined; }
      const reader = response.body?.getReader();
      if (!reader) throw new Error("empty receiving relay response");
      const parts: Uint8Array[] = [];
      let size = 0;
      while (true) {
        const part = await reader.read();
        if (part.done) break;
        size += part.value.byteLength;
        if (size > 384 * 1024) { await reader.cancel(); throw new Error("oversized receiving relay response"); }
        parts.push(part.value);
      }
      const bytes = new Uint8Array(size);
      let offset = 0;
      for (const part of parts) { bytes.set(part, offset); offset += part.length; }
      return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    } finally { clearTimeout(deadline); }
  }

  async register(enrollment: BrowserReceiverEnrollment, registration: ReceivingRegistration, destination: PushSubscriptionJSON): Promise<void> {
    const issuedAt = Date.now();
    const { productId: product } = enrollment.authority;
    const { deviceId, ownerKey } = enrollment;
    const tuple = ["truapi:receiver-binding:v2", 2, product, this.origin, deviceId, ownerKey, issuedAt];
    const signature = hex(await crypto.subtle.sign("Ed25519", enrollment.privateKey, new TextEncoder().encode(JSON.stringify(tuple))));
    await this.request(`/v2/receivers/${deviceId}`, enrollment, "PUT", {
      binding: { v: 2, product, origin: this.origin, deviceId, ownerKey, issuedAt, signature },
      destination: { endpoint: destination.endpoint, keys: destination.keys },
      revision: enrollment.relayRevision, enabled: true,
      watches: registration.watches.filter(watch => watch.expiresAt > BigInt(issuedAt)).map(watch => ({
        watchId: watch.id, genesis: watch.genesis, channel: watch.channel, topics: watch.topics,
        senderKeys: [...watch.senders].sort(),
        expiresAt: Number(watch.expiresAt),
        mutedUntil: Number(watch.mutedUntil > BigInt(Number.MAX_SAFE_INTEGER) ? BigInt(Number.MAX_SAFE_INTEGER) : watch.mutedUntil),
        routeToken: enrollment.routes[watch.id],
      })),
    });
  }

  async revoke(enrollment: BrowserReceiverEnrollment): Promise<void> {
    await this.request(`/v2/receivers/${enrollment.deviceId}`, enrollment, "DELETE", { revision: enrollment.relayRevision });
  }

  async event(enrollment: BrowserReceiverEnrollment, eventId: string): Promise<{
    revision: number; watchId: string; genesis: string; channel: string; topics: string[]; frame: Uint8Array;
  }> {
    if (!/^[A-Za-z0-9._:-]{1,256}$/.test(eventId)) throw new Error("invalid receiver event ID");
    const value = await this.request(`/v2/receivers/${enrollment.deviceId}/events/${encodeURIComponent(eventId)}`, enrollment, "GET") as Record<string, unknown>;
    if (!value || value.v !== 2 || value.revision !== enrollment.relayRevision || typeof value.watchId !== "string" ||
        typeof value.genesis !== "string" || typeof value.channel !== "string" || !Array.isArray(value.topics) ||
        value.topics.length > 4 || !value.topics.every(topic => typeof topic === "string" && /^[a-f0-9]{64}$/.test(topic)) ||
        !/^[a-f0-9]{64}$/.test(value.genesis) || !/^[a-f0-9]{64}$/.test(value.channel) ||
        typeof value.frame !== "string" || value.frame.length > 349528 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value.frame)) {
      throw new Error("invalid retained receiving event");
    }
    const frame = Uint8Array.from(atob(value.frame), value => value.charCodeAt(0));
    if (frame.length > 256 * 1024) throw new Error("oversized receiving frame");
    return { revision: value.revision as number, watchId: value.watchId, genesis: value.genesis,
      channel: value.channel, topics: value.topics as string[], frame };
  }
}
