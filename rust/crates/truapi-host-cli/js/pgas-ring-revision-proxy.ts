// WebSocket proxy for `scripts/pgas-ring-revision-e2e.sh` and native host
// checks. It makes Asset Hub look as if it had not imported the claim's ring
// revision until the host rebuilds its proof, and passes everything else
// through. A host that polls `state_getStorage` reads `MembersSubscriber.RingRoots`
// as empty until it pauses its polling for REBUILD_GAP_MS after polling for at
// least MIN_WAIT_MS; a host that uses
// `state_subscribeStorage` sees RingRoots as empty in its first subscription
// and unchanged in every later one.
// Usage: bun pgas-ring-revision-proxy.ts <upstream-ws-url> <port-file>
// (the listening port is written to <port-file>).
import { writeFileSync } from "node:fs";

// twox128("MembersSubscriber") ++ twox128("RingRoots").
const RING_ROOTS_PREFIX =
  "0xc8d053ab324196afc756c5ae3fbd29178034a4ffd558299b9ac8097424772405";
const REBUILD_GAP_MS = 5_000;
// Just under the host's 30 s wait, so a slow read mid-wait is not taken for a rebuild.
const MIN_WAIT_MS = 25_000;

const [upstreamUrl, portFile] = process.argv.slice(2);
if (!upstreamUrl || !portFile) {
  console.error("usage: pgas-ring-revision-proxy.ts <upstream> <port-file>");
  process.exit(2);
}

let firstHeldRead: number | undefined;
let lastHeldRead: number | undefined;
let released = false;
let ringRootsSubscriptions = 0;

interface Link {
  upstream: WebSocket;
  pending: string[];
  /** Ids of the RingRoots subscribe requests whose subscription is held. */
  heldRequests: Set<string>;
  /** Subscription ids whose RingRoots changes are answered as empty. */
  heldSubscriptions: Set<string>;
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

/** The id of a RingRoots read to answer as empty, or undefined to forward. */
function heldReadId(request: Message): unknown {
  const params = Array.isArray(request.params) ? request.params : [];
  if (request.method !== "state_getStorage" || !isRingRootsKey(params[0])) {
    return undefined;
  }
  if (released) return undefined;
  const now = Date.now();
  if (firstHeldRead === undefined || lastHeldRead === undefined) {
    firstHeldRead = now;
    console.log("proxy: hiding RingRoots until the host rebuilds its proof");
  } else if (
    now - lastHeldRead >= REBUILD_GAP_MS &&
    lastHeldRead - firstHeldRead >= MIN_WAIT_MS
  ) {
    released = true;
    console.log("proxy: the host paused its polling, serving RingRoots again");
    return undefined;
  }
  lastHeldRead = now;
  return request.id;
}

/** Holds the first RingRoots subscription and releases RingRoots on the next. */
function watchSubscribe(link: Link, request: Message) {
  const params = Array.isArray(request.params) ? request.params : [];
  const keys = Array.isArray(params[0]) ? params[0] : [];
  if (
    request.method !== "state_subscribeStorage" ||
    !keys.some(isRingRootsKey)
  ) {
    return;
  }
  ringRootsSubscriptions += 1;
  if (ringRootsSubscriptions === 1) {
    link.heldRequests.add(JSON.stringify(request.id));
    console.log("proxy: hiding RingRoots in the host's first subscription");
  } else if (!released) {
    released = true;
    console.log("proxy: the host subscribed again, serving RingRoots");
  }
}

/** The upstream frame to send on, with held RingRoots changes emptied. */
function rewriteUpstream(link: Link, frame: string): string {
  if (link.heldRequests.size === 0 && link.heldSubscriptions.size === 0) {
    return frame;
  }
  const message = parse(frame);
  if (!message) return frame;
  const requestId = JSON.stringify(message.id);
  if (message.id !== undefined && link.heldRequests.delete(requestId)) {
    link.heldSubscriptions.add(JSON.stringify(message.result));
    return frame;
  }
  const params = message.params as
    | { subscription?: unknown; result?: { changes?: [unknown, unknown][] } }
    | undefined;
  if (
    message.method !== "state_storage" ||
    !link.heldSubscriptions.has(JSON.stringify(params?.subscription)) ||
    !Array.isArray(params?.result?.changes)
  ) {
    return frame;
  }
  params.result.changes = params.result.changes.map(([key, value]) =>
    isRingRootsKey(key) ? [key, null] : [key, value],
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
      heldRequests: new Set(),
      heldSubscriptions: new Set(),
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
      if (request) {
        const id = heldReadId(request);
        if (id !== undefined) {
          ws.send(JSON.stringify({ jsonrpc: "2.0", id, result: null }));
          return;
        }
        watchSubscribe(ws.data, request);
      }
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
