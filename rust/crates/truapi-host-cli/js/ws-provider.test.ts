import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer, type Server as NetServer, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Server } from "bun";
import { createFrameProviderFactory } from "./ws-provider.ts";

const wsProvider = createFrameProviderFactory();

const servers: Server<undefined>[] = [];
const netServers: NetServer[] = [];
const temporaryDirectories: string[] = [];

afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true);
  for (const server of netServers.splice(0)) server.close();
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function unixServer(
  websocket: NonNullable<Parameters<typeof Bun.serve>[0]["websocket"]>,
) {
  const directory = mkdtempSync(join(tmpdir(), "truapi-ws-provider-"));
  temporaryDirectories.push(directory);
  const socketPath = join(directory, "frames.sock");
  const server = Bun.serve({
    unix: socketPath,
    fetch(request, server) {
      if (server.upgrade(request)) return;
      return new Response("websocket upgrade required", { status: 426 });
    },
    websocket,
  });
  servers.push(server);
  return socketPath;
}

async function rawUnixServer(
  connection: (socket: Socket, request: string) => void,
): Promise<string> {
  const directory = mkdtempSync(join(tmpdir(), "truapi-ws-provider-"));
  temporaryDirectories.push(directory);
  const socketPath = join(directory, "frames.sock");
  const server = createServer((socket) => {
    let request = Buffer.alloc(0);
    const readHandshake = (chunk: Buffer) => {
      request = Buffer.concat([request, chunk]);
      const boundary = request.indexOf("\r\n\r\n");
      if (boundary < 0) return;
      socket.off("data", readHandshake);
      connection(socket, request.subarray(0, boundary).toString("utf8"));
    };
    socket.on("data", readHandshake);
  });
  netServers.push(server);
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(socketPath, resolve);
  });
  return socketPath;
}

function acceptKey(request: string): string {
  const key = request.match(/^Sec-WebSocket-Key:\s*(.+)$/im)?.[1]?.trim();
  if (!key) throw new Error("request did not include Sec-WebSocket-Key");
  return createHash("sha1")
    .update(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")
    .digest("base64");
}

function upgradeResponse(request: string, accept = acceptKey(request)): string {
  return [
    "HTTP/1.1 101 Switching Protocols",
    "Upgrade: websocket",
    "Connection: Upgrade",
    `Sec-WebSocket-Accept: ${accept}`,
    "",
    "",
  ].join("\r\n");
}

function serverFrame(fin: boolean, opcode: number, payload: number[]): Buffer {
  if (payload.length > 125)
    throw new Error("test helper only supports short frames");
  return Buffer.from([(fin ? 0x80 : 0) | opcode, payload.length, ...payload]);
}

function readMaskedClientFrame(bytes: Buffer): {
  opcode: number;
  payload: number[];
} {
  const opcode = bytes[0]! & 0x0f;
  const length = bytes[1]! & 0x7f;
  if ((bytes[1]! & 0x80) === 0 || length > 125) {
    throw new Error("expected a short masked client frame");
  }
  const mask = bytes.subarray(2, 6);
  const payload = Array.from(bytes.subarray(6, 6 + length), (byte, index) => {
    return byte ^ mask[index % 4]!;
  });
  return { opcode, payload };
}

describe("wsProvider Unix-domain WebSockets", () => {
  test("buffers and round-trips a binary frame", async () => {
    const socketPath = unixServer({
      message(websocket, message) {
        websocket.send(message);
      },
    });
    const provider = wsProvider(`ws+unix:${socketPath}`);
    const received = new Promise<Uint8Array>((resolve) => {
      provider.subscribe(resolve);
    });

    provider.postMessage(Uint8Array.from([1, 2, 3, 4]));
    await provider.opened;

    const echoed = await received;
    expect(Array.from(echoed)).toEqual([1, 2, 3, 4]);
    expect(echoed.byteOffset).toBe(0);
    expect(echoed.buffer.byteLength).toBe(echoed.byteLength);
    provider.dispose();
  });

  test("serializes a burst of product frames", async () => {
    const socketPath = unixServer({
      message(websocket, message) {
        websocket.send(message);
      },
    });
    const provider = wsProvider(`ws+unix:${socketPath}`);
    const expected = 128;
    let received = 0;
    const completed = new Promise<void>((resolve) => {
      provider.subscribe(() => {
        received += 1;
        if (received === expected) resolve();
      });
    });

    await provider.opened;
    for (let index = 0; index < expected; index += 1) {
      provider.postMessage(new Uint8Array(2048).fill(index));
    }

    await completed;
    expect(received).toBe(expected);
    provider.dispose();
  });

  test("round-trips a large product frame", async () => {
    const socketPath = unixServer({
      message(websocket, message) {
        websocket.send(message);
      },
    });
    const provider = wsProvider(`ws+unix:${socketPath}`);
    const received = new Promise<Uint8Array>((resolve) => {
      provider.subscribe(resolve);
    });
    const payload = new Uint8Array(256 * 1024);
    payload[0] = 11;
    payload[payload.length - 1] = 22;

    await provider.opened;
    provider.postMessage(payload);

    const echoed = await received;
    expect(echoed.length).toBe(payload.length);
    expect([echoed[0], echoed[echoed.length - 1]]).toEqual([11, 22]);
    provider.dispose();
  });

  test("reports a server close to subscribers", async () => {
    const socketPath = unixServer({
      open(websocket) {
        websocket.close(1000, "done");
      },
      message() {},
    });
    const provider = wsProvider(`ws+unix:${socketPath}`);
    const closed = new Promise<Error>((resolve) => {
      provider.subscribeClose(resolve);
    });

    await provider.opened;

    expect((await closed).message).toBe("websocket closed");
    provider.dispose();
  });

  test("reassembles fragments and answers ping frames", async () => {
    let resolvePong!: (frame: { opcode: number; payload: number[] }) => void;
    const pong = new Promise<{ opcode: number; payload: number[] }>(
      (resolve) => {
        resolvePong = resolve;
      },
    );
    const socketPath = await rawUnixServer((socket, request) => {
      socket.write(
        Buffer.concat([
          Buffer.from(upgradeResponse(request)),
          serverFrame(false, 0x2, [1, 2]),
          serverFrame(true, 0x9, [9]),
          serverFrame(true, 0x0, [3, 4]),
        ]),
      );
      socket.once("data", (bytes) => {
        resolvePong(readMaskedClientFrame(Buffer.from(bytes)));
      });
    });
    const provider = wsProvider(`ws+unix:${socketPath}`);
    const received = new Promise<Uint8Array>((resolve) => {
      provider.subscribe(resolve);
    });

    await provider.opened;

    expect(Array.from(await received)).toEqual([1, 2, 3, 4]);
    expect(await pong).toEqual({ opcode: 0xa, payload: [9] });
    provider.dispose();
  });

  test("rejects an invalid upgrade accept key", async () => {
    const socketPath = await rawUnixServer((socket, request) => {
      socket.write(upgradeResponse(request, "not-the-right-key"));
    });
    const provider = wsProvider(`ws+unix:${socketPath}`);

    await expect(provider.opened).rejects.toThrow("invalid accept key");
    provider.dispose();
  });
});

describe("wsProvider TCP WebSockets", () => {
  test("a captured factory bypasses later WebSocket gates on every connection", async () => {
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch(request, server) {
        if (server.upgrade(request)) return;
        return new Response("websocket upgrade required", { status: 426 });
      },
      websocket: {
        message(websocket, message) {
          websocket.send(message);
        },
      },
    });
    servers.push(server);
    const createProvider = createFrameProviderFactory();
    const NativeWebSocket = globalThis.WebSocket;
    globalThis.WebSocket = new Proxy(NativeWebSocket, {
      construct() {
        throw new Error("host transport entered the product WebSocket gate");
      },
    });
    try {
      for (const payload of [
        [5, 6, 7],
        [8, 9],
      ]) {
        const provider = createProvider(`ws://127.0.0.1:${server.port}`);
        try {
          const received = new Promise<Uint8Array>((resolve) =>
            provider.subscribe(resolve),
          );
          await provider.opened;
          provider.postMessage(Uint8Array.from(payload));
          expect(Array.from(await received)).toEqual(payload);
        } finally {
          provider.dispose();
        }
      }
    } finally {
      globalThis.WebSocket = NativeWebSocket;
    }
  });

  test("keeps using the native WebSocket transport", async () => {
    const server = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch(request, server) {
        if (server.upgrade(request)) return;
        return new Response("websocket upgrade required", { status: 426 });
      },
      websocket: {
        message(websocket, message) {
          websocket.send(message);
        },
      },
    });
    servers.push(server);
    const provider = wsProvider(`ws://127.0.0.1:${server.port}`);
    const received = new Promise<Uint8Array>((resolve) => {
      provider.subscribe(resolve);
    });

    await provider.opened;
    provider.postMessage(Uint8Array.from([5, 6, 7]));

    expect(Array.from(await received)).toEqual([5, 6, 7]);
    provider.dispose();
  });
});
