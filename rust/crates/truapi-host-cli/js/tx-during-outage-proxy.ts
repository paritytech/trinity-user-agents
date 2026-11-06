// WebSocket proxy for `scripts/tx-during-outage-e2e.sh`. It closes every
// connection it carries and refuses new ones for the given window, in one of
// these modes:
//
// - `close`: a frame carrying the trigger closes everything at once.
// - `header`: the trigger arms the cut, and the next `chainHead_v1_header`
//   closes everything under it, so the connection drops partway through
//   reading the current block.
// - `metadata`: the trigger is not needed. The first runtime call fetching
//   metadata closes everything under it, so the connection drops in the
//   middle of a cold metadata download.
//
// After the window it proxies normally again. On the cut it writes
// `<port-file>.cut`, so the caller can tell a cut happened.
// Usage: bun tx-during-outage-proxy.ts <upstream-ws-url> <port-file> <trigger> <outage-ms> <close|header|metadata>
import { writeFileSync } from "node:fs";

const [upstreamUrl, portFile, trigger, outageArg, mode] = process.argv.slice(2);
const outageMs = Number(outageArg);
if (
  !upstreamUrl ||
  !portFile ||
  !trigger ||
  !Number.isFinite(outageMs) ||
  !["close", "header", "metadata"].includes(mode ?? "")
) {
  console.error(
    "usage: tx-during-outage-proxy.ts <upstream> <port-file> <trigger> <outage-ms> <close|header|metadata>",
  );
  process.exit(2);
}

interface Link {
  upstream: WebSocket;
  pending: string[];
}

const links = new Set<Bun.ServerWebSocket<Link>>();
let cut = false;
/** In `header` mode, set once the trigger arms the cut. */
let armed = false;
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

function closeEverything(under: string) {
  downUntil = Date.now() + outageMs;
  writeFileSync(`${portFile}.cut`, under);
  console.log(
    `proxy: closing ${links.size} connections ${under} and refusing new ones for ${outageMs} ms`,
  );
  for (const link of links) {
    link.data.upstream.close();
    link.close();
  }
}

const server = Bun.serve<Link>({
  hostname: "127.0.0.1",
  port: 0,
  fetch(request, server) {
    if (down()) {
      return new Response("outage", { status: 503 });
    }
    const upstream = new WebSocket(upstreamUrl);
    if (server.upgrade(request, { data: { upstream, pending: [] } })) {
      return undefined;
    }
    upstream.close();
    return new Response("expected a WebSocket upgrade", { status: 400 });
  },
  websocket: {
    maxPayloadLength: 64 * 1024 * 1024,
    open(ws) {
      links.add(ws);
      const { upstream, pending } = ws.data;
      upstream.addEventListener("open", () => {
        for (const frame of pending.splice(0)) upstream.send(frame);
      });
      upstream.addEventListener("message", (event) => ws.send(String(event.data)));
      upstream.addEventListener("close", () => ws.close());
      upstream.addEventListener("error", () => ws.close());
    },
    message(ws, message) {
      const frame = String(message);
      const request = parse(frame);
      if (!cut && mode !== "metadata" && frame.includes(trigger)) {
        cut = true;
        if (mode === "close") {
          closeEverything("on the trigger");
          return;
        }
        armed = true;
        console.log("proxy: armed, the next chainHead_v1_header closes every connection");
      }
      if (
        mode === "metadata" &&
        !cut &&
        request?.method === "chainHead_v1_call" &&
        frame.includes("Metadata_metadata")
      ) {
        cut = true;
        closeEverything("under a metadata call");
        return;
      }
      if (armed && request?.method === "chainHead_v1_header") {
        armed = false;
        closeEverything("under chainHead_v1_header");
        return;
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
