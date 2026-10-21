// WebSocket proxy for `e2e/cli/chain-head-failure-cause.sh`. It closes the
// connection when the host sends `chainHead_v1_storage` instead of forwarding
// it, so the call is always in flight when its transport closes.
// Usage: bun chain-head-failure-cause-proxy.ts <upstream-ws-url> <port-file>
// (the listening port is written to <port-file>).
import { writeFileSync } from "node:fs";

const [upstreamUrl, portFile] = process.argv.slice(2);
if (!upstreamUrl || !portFile) {
  console.error(
    "usage: chain-head-failure-cause-proxy.ts <upstream> <port-file>",
  );
  process.exit(2);
}

interface Link {
  upstream: WebSocket;
  pending: string[];
}

const server = Bun.serve<Link>({
  hostname: "127.0.0.1",
  port: 0,
  fetch(request, server) {
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
      const { upstream, pending } = ws.data;
      upstream.addEventListener("open", () => {
        for (const frame of pending.splice(0)) upstream.send(frame);
      });
      upstream.addEventListener("message", (event) => {
        ws.send(String(event.data));
      });
      upstream.addEventListener("close", () => ws.close());
      upstream.addEventListener("error", () => ws.close());
    },
    message(ws, message) {
      const frame = String(message);
      if (frame.includes('"chainHead_v1_storage"')) {
        console.log(
          "proxy: holding chainHead_v1_storage and closing the connection",
        );
        ws.data.upstream.close();
        ws.close();
        return;
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
