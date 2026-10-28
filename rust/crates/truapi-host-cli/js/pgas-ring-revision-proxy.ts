// WebSocket proxy for `scripts/pgas-ring-revision-e2e.sh` and native host
// checks. It makes Asset Hub look as if it had skipped the claim's ring
// revision, and passes everything else through.
//
// The host's first `MembersSubscriber.RingRoots` read, or its first RingRoots
// subscription, is answered with one root at revision u32::MAX, so whatever
// revision the proof names reads as pruned. A host that rebuilds its proof
// reads RingRoots again, and that second read is the release: it and every
// later one are served unchanged. A host that does not rebuild never reads
// again, so it is never released.
//
// Each step is appended to <events-file> as one line: `pruned` when the forged
// roots are served, `rebuilt` when the host reads RingRoots again, and
// `submitted` when it submits an extrinsic after that.
//
// Usage: bun pgas-ring-revision-proxy.ts <upstream-ws-url> <port-file> <events-file>
// (the listening port is written to <port-file>).
import { appendFileSync, writeFileSync } from "node:fs";

// twox128("MembersSubscriber") ++ twox128("RingRoots").
const RING_ROOTS_PREFIX =
  "0xc8d053ab324196afc756c5ae3fbd29178034a4ffd558299b9ac8097424772405";
// Offset of `revision` in one RingRoots record: it follows the 288-byte root,
// as in truapi's `paseo-next-asset-hub-ring-5-roots.scale` fixture.
const REVISION_OFFSET = 288;
const NEWEST_REVISION = 0xffff_ffff;
const SUBMIT_METHODS = new Set([
  "author_submitExtrinsic",
  "author_submitAndWatchExtrinsic",
]);

const [upstreamUrl, portFile, eventsFile] = process.argv.slice(2);
if (!upstreamUrl || !portFile || !eventsFile) {
  console.error(
    "usage: pgas-ring-revision-proxy.ts <upstream> <port-file> <events-file>",
  );
  process.exit(2);
}

type Phase = "waiting" | "pruned" | "rebuilt";
let phase: Phase = "waiting";
let submitted = false;

function record(event: string, detail: string) {
  appendFileSync(eventsFile, `${event}\n`);
  console.log(`proxy: ${detail}`);
}

interface Link {
  upstream: WebSocket;
  pending: string[];
  /** Ids of RingRoots reads whose response is served as pruned. */
  prunedReads: Set<string>;
  /** Ids of RingRoots subscribe requests whose subscription is served as pruned. */
  prunedSubscribeRequests: Set<string>;
  /** Subscription ids whose RingRoots changes are served as pruned. */
  prunedSubscriptions: Set<string>;
}

interface Message {
  id?: unknown;
  method?: string;
  params?: unknown;
  result?: unknown;
}

function parse(frame: string): Message | undefined {
  try {
    const message: unknown = JSON.parse(frame);
    return typeof message === "object" &&
      message !== null &&
      !Array.isArray(message)
      ? (message as Message)
      : undefined;
  } catch {
    return undefined;
  }
}

const isRingRootsKey = (key: unknown) =>
  typeof key === "string" && key.startsWith(RING_ROOTS_PREFIX);

/** A SCALE compact length and the offset of what follows it. */
function decodeCompact(bytes: Buffer): { value: number; offset: number } {
  const mode = bytes[0] & 0b11;
  if (mode === 0) return { value: bytes[0] >> 2, offset: 1 };
  if (mode === 1) return { value: bytes.readUInt16LE(0) >> 2, offset: 2 };
  if (mode === 2) return { value: bytes.readUInt32LE(0) >>> 2, offset: 4 };
  throw new Error("RingRoots holds more records than a proxy check expects");
}

/** `value` reduced to its last record, re-numbered as the newest revision. */
function prunedRoots(value: unknown): unknown {
  if (typeof value !== "string") return value;
  const bytes = Buffer.from(value.slice(2), "hex");
  const { value: count, offset } = decodeCompact(bytes);
  if (count === 0) return value;
  const size = (bytes.length - offset) / count;
  if (!Number.isInteger(size) || size < REVISION_OFFSET + 4) {
    console.error(
      `proxy: a RingRoots record of ${size} bytes has no revision at ${REVISION_OFFSET}`,
    );
    process.exit(1);
  }
  const last = Buffer.from(bytes.subarray(bytes.length - size));
  last.writeUInt32LE(NEWEST_REVISION, REVISION_OFFSET);
  return `0x${Buffer.concat([Buffer.from([1 << 2]), last]).toString("hex")}`;
}

/** Whether this RingRoots read or subscription is served as pruned. */
function prunesRingRoots(): boolean {
  if (phase === "waiting") {
    phase = "pruned";
    record(
      "pruned",
      "serving RingRoots as pruned until the host rebuilds its proof",
    );
    return true;
  }
  if (phase === "pruned") {
    phase = "rebuilt";
    record(
      "rebuilt",
      "the host read RingRoots again for its rebuilt proof, serving it unchanged",
    );
  }
  return false;
}

/** Notes the request on `link` if its RingRoots answer is to be pruned. */
function watchRequest(link: Link, request: Message) {
  const params = Array.isArray(request.params) ? request.params : [];
  const id = JSON.stringify(request.id);
  if (request.method === "state_getStorage" && isRingRootsKey(params[0])) {
    if (prunesRingRoots()) link.prunedReads.add(id);
  } else if (
    request.method === "state_subscribeStorage" &&
    Array.isArray(params[0]) &&
    params[0].some(isRingRootsKey)
  ) {
    if (prunesRingRoots()) link.prunedSubscribeRequests.add(id);
  } else if (
    request.method !== undefined &&
    SUBMIT_METHODS.has(request.method) &&
    phase === "rebuilt" &&
    !submitted
  ) {
    submitted = true;
    record("submitted", "the host submitted its rebuilt proof");
  }
}

/** The upstream frame to send on, with pruned RingRoots answers rewritten. */
function rewriteUpstream(link: Link, frame: string): string {
  if (
    link.prunedReads.size === 0 &&
    link.prunedSubscribeRequests.size === 0 &&
    link.prunedSubscriptions.size === 0
  ) {
    return frame;
  }
  const message = parse(frame);
  if (!message) return frame;
  const requestId = JSON.stringify(message.id);
  if (message.id !== undefined && link.prunedReads.delete(requestId)) {
    return JSON.stringify({ ...message, result: prunedRoots(message.result) });
  }
  if (
    message.id !== undefined &&
    link.prunedSubscribeRequests.delete(requestId)
  ) {
    link.prunedSubscriptions.add(JSON.stringify(message.result));
    return frame;
  }
  const params = message.params as
    | { subscription?: unknown; result?: { changes?: [unknown, unknown][] } }
    | undefined;
  if (
    message.method !== "state_storage" ||
    !link.prunedSubscriptions.has(JSON.stringify(params?.subscription)) ||
    !Array.isArray(params?.result?.changes)
  ) {
    return frame;
  }
  params.result.changes = params.result.changes.map(([key, value]) =>
    isRingRootsKey(key) ? [key, prunedRoots(value)] : [key, value],
  );
  return JSON.stringify(message);
}

const server = Bun.serve<Link>({
  hostname: "127.0.0.1",
  port: 0,
  fetch(request, server) {
    const upstream = new WebSocket(upstreamUrl);
    const data: Link = {
      upstream,
      pending: [],
      prunedReads: new Set(),
      prunedSubscribeRequests: new Set(),
      prunedSubscriptions: new Set(),
    };
    if (server.upgrade(request, { data })) {
      return undefined;
    }
    upstream.close();
    return new Response("expected a WebSocket upgrade", { status: 400 });
  },
  websocket: {
    maxPayloadLength: 64 * 1024 * 1024,
    open(ws) {
      const { upstream, pending } = ws.data;
      upstream.addEventListener("open", () => {
        for (const frame of pending.splice(0)) upstream.send(frame);
      });
      upstream.addEventListener("message", (event) => {
        ws.send(rewriteUpstream(ws.data, String(event.data)));
      });
      upstream.addEventListener("close", () => ws.close());
      upstream.addEventListener("error", () => ws.close());
    },
    message(ws, message) {
      const frame = String(message);
      const request = parse(frame);
      if (request) watchRequest(ws.data, request);
      if (ws.data.upstream.readyState === WebSocket.OPEN) {
        ws.data.upstream.send(frame);
      } else {
        ws.data.pending.push(frame);
      }
    },
    close(ws) {
      ws.data.upstream.close();
    },
  },
});

writeFileSync(portFile, String(server.port));
console.log(`proxy: 127.0.0.1:${server.port} -> ${upstreamUrl}`);
