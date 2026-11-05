// WebSocket proxy for `scripts/tx-during-outage-e2e.sh`. When a frame carries
// the trigger key, an outage starts for the given window, in one of two modes:
//
// - `close`: every open connection closes and new ones are refused.
// - `stop`: connections stay open, every follow they carry is stopped, and a
//   follow opened during the window is answered with an error after a delay,
//   the way a node whose upstream lost DNS refuses to follow.
//
// After the window it proxies normally again.
// Usage: bun tx-during-outage-proxy.ts <upstream-ws-url> <port-file> <trigger> <outage-ms> <close|stop>
import { writeFileSync } from "node:fs";

const [upstreamUrl, portFile, trigger, outageArg, mode] = process.argv.slice(2);
const outageMs = Number(outageArg);
if (
  !upstreamUrl ||
  !portFile ||
  !trigger ||
  !Number.isFinite(outageMs) ||
  (mode !== "close" && mode !== "stop")
) {
  console.error(
    "usage: tx-during-outage-proxy.ts <upstream> <port-file> <trigger> <outage-ms> <close|stop>",
  );
  process.exit(2);
}
/** How long a follow opened during a `stop` outage waits before it is refused. */
const REFUSAL_DELAY_MS = 1_500;

interface Link {
  upstream: WebSocket;
  pending: string[];
  /** Request id to method, for requests still waiting on an answer. */
  methods: Map<unknown, string>;
  /** Follow subscriptions this connection carries. */
  follows: Set<string>;
}

const links = new Set<Bun.ServerWebSocket<Link>>();
let cut = false;
let downUntil = 0;
const down = () => Date.now() < downUntil;

function parse(frame: string): Record<string, unknown> | undefined {
  try {
    const value = JSON.parse(frame);
    return typeof value === "object" && value !== null ? value : undefined;
  } catch {
    return undefined;
  }
}

function startOutage() {
  cut = true;
  downUntil = Date.now() + outageMs;
  if (mode === "close") {
    console.log(
      `proxy: closing ${links.size} connections and refusing new ones for ${outageMs} ms`,
    );
    for (const link of links) {
      link.data.upstream.close();
      link.close();
    }
    return;
  }
  let stopped = 0;
  for (const link of links) {
    for (const subscription of link.data.follows) {
      link.send(
        JSON.stringify({
          jsonrpc: "2.0",
          method: "chainHead_v1_followEvent",
          params: { subscription, result: { event: "stop" } },
        }),
      );
      stopped++;
    }
    link.data.follows.clear();
  }
  console.log(
    `proxy: stopped ${stopped} follows and refusing new ones for ${outageMs} ms`,
  );
}

const server = Bun.serve<Link>({
  hostname: "127.0.0.1",
  port: 0,
  fetch(request, server) {
    if (mode === "close" && down()) {
      return new Response("outage", { status: 503 });
    }
    const upstream = new WebSocket(upstreamUrl);
    const data: Link = {
      upstream,
      pending: [],
      methods: new Map(),
      follows: new Set(),
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
      links.add(ws);
      const { upstream, pending, methods, follows } = ws.data;
      upstream.addEventListener("open", () => {
        for (const frame of pending.splice(0)) upstream.send(frame);
      });
      upstream.addEventListener("message", (event) => {
        const frame = String(event.data);
        const message = parse(frame);
        if (message && "id" in message) {
          const method = methods.get(message.id);
          methods.delete(message.id);
          if (
            method === "chainHead_v1_follow" &&
            typeof message.result === "string"
          ) {
            follows.add(message.result);
          }
        }
        ws.send(frame);
      });
      upstream.addEventListener("close", () => ws.close());
      upstream.addEventListener("error", () => ws.close());
    },
    message(ws, message) {
      const frame = String(message);
      if (!cut && frame.includes(trigger)) {
        startOutage();
        if (mode === "close") return;
      }
      const request = parse(frame);
      if (
        mode === "stop" &&
        down() &&
        request?.method === "chainHead_v1_follow"
      ) {
        setTimeout(() => {
          ws.send(
            JSON.stringify({
              jsonrpc: "2.0",
              id: request.id,
              error: { code: -32603, message: "upstream unreachable" },
            }),
          );
        }, REFUSAL_DELAY_MS);
        return;
      }
      if (request && "id" in request && typeof request.method === "string") {
        ws.data.methods.set(request.id, request.method);
      }
      if (ws.data.upstream.readyState === WebSocket.OPEN) {
        ws.data.upstream.send(frame);
      } else {
        ws.data.pending.push(frame);
      }
    },
    close(ws) {
      links.delete(ws);
      ws.data.upstream.close();
    },
  },
});

writeFileSync(portFile, String(server.port));
console.log(`proxy: 127.0.0.1:${server.port} -> ${upstreamUrl} (${mode})`);
