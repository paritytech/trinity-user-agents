// WebSocket `WireProvider` for @parity/truapi: one binary WebSocket message
// per SCALE protocol frame. TCP endpoints reuse the native provider factory from
// the client package; Unix endpoints perform the same RFC 6455 protocol over a
// filesystem socket, which no browser needs and only this CLI speaks.
import { createHash, randomBytes } from "node:crypto";
import { connect } from "node:net";
import {
  createWebSocketProviderFactory,
  type WebSocketWireProvider,
} from "../../../../js/packages/truapi/src/transport.ts";

const UNIX_WS_PREFIX = "ws+unix:";
const WEBSOCKET_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_MESSAGE_BYTES = 64 * 1024 * 1024;
const MAX_HANDSHAKE_BYTES = 16 * 1024;

/** Capture native socket APIs before the runner installs network gates. */
export function createFrameProviderFactory(): (
  url: string,
) => WebSocketWireProvider {
  const createTcpProvider = createWebSocketProviderFactory();
  return (url) =>
    url.startsWith(UNIX_WS_PREFIX)
      ? unixWsProvider(parseUnixSocketPath(url))
      : createTcpProvider(url);
}

function parseUnixSocketPath(url: string): string {
  const path = url.slice(UNIX_WS_PREFIX.length);
  if (!path.startsWith("/")) {
    throw new Error(`invalid Unix WebSocket endpoint: ${url}`);
  }
  return path;
}

function unixWsProvider(socketPath: string): WebSocketWireProvider {
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  const pending: Uint8Array[] = [];
  const websocketKey = randomBytes(16).toString("base64");
  const expectedAccept = createHash("sha1")
    .update(websocketKey + WEBSOCKET_GUID)
    .digest("base64");
  const socket = connect(socketPath);
  let handshakeBuffer = Buffer.alloc(0);
  let frameBuffer: Buffer = Buffer.alloc(0);
  let fragmentedOpcode: number | undefined;
  let fragmentedPayloads: Buffer[] = [];
  let fragmentedLength = 0;
  let handshakeComplete = false;
  let openedSettled = false;
  let closed = false;
  let disposed = false;
  let sentClose = false;
  let writing = false;
  let endAfterWrites = false;
  let lastWriteLength = 0;
  let largestWriteLength = 0;
  let completedWrites = 0;
  const writeQueue: Buffer[] = [];

  let resolveOpened!: () => void;
  let rejectOpened!: (error: Error) => void;
  const opened = new Promise<void>((resolve, reject) => {
    resolveOpened = resolve;
    rejectOpened = reject;
  });

  const settleOpenError = (error: Error) => {
    if (openedSettled) return;
    openedSettled = true;
    rejectOpened(error);
  };
  const notifyClosed = (error: Error) => {
    if (closed) return;
    closed = true;
    settleOpenError(error);
    if (!disposed) {
      for (const listener of closeListeners) listener(error);
    }
  };
  const fail = (error: Error, closeCode = 1002) => {
    writeQueue.length = 0;
    const sendClose = handshakeComplete && !sentClose && socket.writable;
    if (sendClose) {
      sentClose = true;
      endAfterWrites = true;
      writeQueue.push(clientFrame(0x8, closePayload(closeCode)));
    }
    notifyClosed(error);
    if (sendClose) flushWrites();
    else socket.destroy();
  };
  const flushWrites = () => {
    if (writing || !handshakeComplete) return;
    if (!socket.writable) {
      writeQueue.length = 0;
      return;
    }
    const frame = writeQueue.shift();
    if (!frame) {
      if (endAfterWrites && socket.writable) socket.end();
      return;
    }
    writing = true;
    lastWriteLength = frame.length;
    largestWriteLength = Math.max(largestWriteLength, frame.length);
    socket.write(frame, () => {
      completedWrites += 1;
      writing = false;
      flushWrites();
    });
  };
  const writeWebSocketFrame = (opcode: number, payload: Uint8Array) => {
    if (closed || !socket.writable) return;
    writeQueue.push(clientFrame(opcode, payload));
    flushWrites();
  };
  const emitMessage = (payload: Buffer) => {
    // Match native WebSocket delivery: consumers may pass `.buffer` to the
    // SCALE decoder, so never expose a view containing frame-header bytes.
    const bytes = new Uint8Array(payload.length);
    bytes.set(payload);
    for (const listener of listeners) listener(bytes);
  };
  const finishFragment = (payload: Buffer) => {
    fragmentedPayloads.push(payload);
    fragmentedLength += payload.length;
    if (fragmentedLength > MAX_MESSAGE_BYTES) {
      fail(new Error("websocket message exceeds maximum size"), 1009);
      return;
    }
    emitMessage(Buffer.concat(fragmentedPayloads, fragmentedLength));
    fragmentedOpcode = undefined;
    fragmentedPayloads = [];
    fragmentedLength = 0;
  };
  const handleFrame = (fin: boolean, opcode: number, payload: Buffer) => {
    if (opcode >= 0x8) {
      if (!fin || payload.length > 125) {
        fail(new Error("invalid websocket control frame"));
        return;
      }
      if (opcode === 0x8) {
        if (payload.length === 1) {
          fail(new Error("invalid websocket close frame"));
          return;
        }
        if (!sentClose) {
          sentClose = true;
          writeWebSocketFrame(0x8, payload);
        }
        endAfterWrites = true;
        notifyClosed(new Error("websocket closed"));
        flushWrites();
      } else if (opcode === 0x9) {
        writeWebSocketFrame(0xa, payload);
      } else if (opcode !== 0xa) {
        fail(new Error(`unsupported websocket opcode ${opcode}`));
      }
      return;
    }

    if (opcode === 0x0) {
      if (fragmentedOpcode === undefined) {
        fail(new Error("unexpected websocket continuation frame"));
        return;
      }
      if (fin) {
        finishFragment(payload);
      } else {
        fragmentedPayloads.push(payload);
        fragmentedLength += payload.length;
        if (fragmentedLength > MAX_MESSAGE_BYTES) {
          fail(new Error("websocket message exceeds maximum size"), 1009);
        }
      }
      return;
    }

    if (opcode !== 0x1 && opcode !== 0x2) {
      fail(new Error(`unsupported websocket opcode ${opcode}`));
      return;
    }
    if (fragmentedOpcode !== undefined) {
      fail(new Error("new websocket message started before continuation"));
      return;
    }
    if (fin) {
      emitMessage(payload);
    } else {
      fragmentedOpcode = opcode;
      fragmentedPayloads = [payload];
      fragmentedLength = payload.length;
    }
  };
  const parseFrames = () => {
    while (!closed) {
      if (frameBuffer.length < 2) return;
      const first = frameBuffer[0]!;
      const second = frameBuffer[1]!;
      if ((first & 0x70) !== 0) {
        fail(new Error("websocket extensions were not negotiated"));
        return;
      }
      if ((second & 0x80) !== 0) {
        fail(new Error("server websocket frames must not be masked"));
        return;
      }

      let headerLength = 2;
      let payloadLength = second & 0x7f;
      if (payloadLength === 126) {
        if (frameBuffer.length < 4) return;
        payloadLength = frameBuffer.readUInt16BE(2);
        headerLength = 4;
      } else if (payloadLength === 127) {
        if (frameBuffer.length < 10) return;
        const length = frameBuffer.readBigUInt64BE(2);
        if (length > BigInt(Number.MAX_SAFE_INTEGER)) {
          fail(new Error("websocket frame length is not representable"), 1009);
          return;
        }
        payloadLength = Number(length);
        headerLength = 10;
      }
      if (payloadLength > MAX_MESSAGE_BYTES) {
        fail(new Error("websocket frame exceeds maximum size"), 1009);
        return;
      }
      if (frameBuffer.length < headerLength + payloadLength) return;
      const payload = frameBuffer.subarray(
        headerLength,
        headerLength + payloadLength,
      );
      frameBuffer = frameBuffer.subarray(headerLength + payloadLength);
      handleFrame((first & 0x80) !== 0, first & 0x0f, payload);
    }
  };
  const completeHandshake = (header: string, remaining: Buffer) => {
    const lines = header.split("\r\n");
    const status = lines.shift() ?? "";
    if (!/^HTTP\/1\.[01] 101(?: |$)/.test(status)) {
      fail(
        new Error(`websocket upgrade failed: ${status || "empty response"}`),
      );
      return;
    }
    const headers = new Map<string, string>();
    for (const line of lines) {
      const separator = line.indexOf(":");
      if (separator <= 0) continue;
      headers.set(
        line.slice(0, separator).trim().toLowerCase(),
        line.slice(separator + 1).trim(),
      );
    }
    if (headers.get("upgrade")?.toLowerCase() !== "websocket") {
      fail(new Error("websocket upgrade response is missing Upgrade"));
      return;
    }
    const connection = headers.get("connection")?.toLowerCase() ?? "";
    if (!connection.split(",").some((value) => value.trim() === "upgrade")) {
      fail(new Error("websocket upgrade response is missing Connection"));
      return;
    }
    if (headers.get("sec-websocket-accept") !== expectedAccept) {
      fail(new Error("websocket upgrade response has an invalid accept key"));
      return;
    }

    handshakeComplete = true;
    openedSettled = true;
    resolveOpened();
    for (const frame of pending.splice(0)) {
      writeWebSocketFrame(0x2, frame);
    }
    frameBuffer = remaining;
    parseFrames();
  };

  socket.on("connect", () => {
    socket.write(
      [
        "GET / HTTP/1.1",
        "Host: localhost",
        "Upgrade: websocket",
        "Connection: Upgrade",
        `Sec-WebSocket-Key: ${websocketKey}`,
        "Sec-WebSocket-Version: 13",
        "",
        "",
      ].join("\r\n"),
    );
  });
  socket.on("data", (chunk) => {
    if (closed) return;
    const bytes = Buffer.from(chunk);
    if (handshakeComplete) {
      frameBuffer = Buffer.concat([frameBuffer, bytes]);
      parseFrames();
      return;
    }

    handshakeBuffer = Buffer.concat([handshakeBuffer, bytes]);
    if (handshakeBuffer.length > MAX_HANDSHAKE_BYTES) {
      fail(new Error("websocket upgrade response is too large"));
      return;
    }
    const boundary = handshakeBuffer.indexOf("\r\n\r\n");
    if (boundary < 0) return;
    const header = handshakeBuffer.subarray(0, boundary).toString("utf8");
    const remaining = handshakeBuffer.subarray(boundary + 4);
    handshakeBuffer = Buffer.alloc(0);
    completeHandshake(header, remaining);
  });
  socket.on("error", (error) => {
    notifyClosed(
      new Error(
        `websocket error: ${error.message} ` +
          `(last write ${lastWriteLength} bytes, largest ${largestWriteLength}, ` +
          `${completedWrites} completed, ${writeQueue.length} queued)`,
        { cause: error },
      ),
    );
  });
  socket.on("close", () => {
    notifyClosed(new Error("websocket closed"));
  });

  return {
    opened,
    postMessage(message: Uint8Array) {
      if (closed || disposed) return;
      if (handshakeComplete) writeWebSocketFrame(0x2, message);
      else pending.push(message);
    },
    subscribe(cb: (message: Uint8Array) => void) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    subscribeClose(cb: (error: Error) => void) {
      closeListeners.add(cb);
      return () => closeListeners.delete(cb);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      pending.length = 0;
      if (handshakeComplete && !sentClose) {
        sentClose = true;
        endAfterWrites = true;
        writeWebSocketFrame(0x8, closePayload(1000));
      } else {
        socket.destroy();
      }
    },
  };
}

function closePayload(code: number): Buffer {
  const payload = Buffer.allocUnsafe(2);
  payload.writeUInt16BE(code);
  return payload;
}

function clientFrame(opcode: number, value: Uint8Array): Buffer {
  const payload = Buffer.from(value.buffer, value.byteOffset, value.byteLength);
  const lengthBytes =
    payload.length < 126 ? 0 : payload.length <= 0xffff ? 2 : 8;
  const headerLength = 2 + lengthBytes + 4;
  const frame = Buffer.allocUnsafe(headerLength + payload.length);
  frame[0] = 0x80 | opcode;
  if (lengthBytes === 0) {
    frame[1] = 0x80 | payload.length;
  } else if (lengthBytes === 2) {
    frame[1] = 0x80 | 126;
    frame.writeUInt16BE(payload.length, 2);
  } else {
    frame[1] = 0x80 | 127;
    frame.writeBigUInt64BE(BigInt(payload.length), 2);
  }
  const maskOffset = 2 + lengthBytes;
  const mask = randomBytes(4);
  mask.copy(frame, maskOffset);
  for (let index = 0; index < payload.length; index += 1) {
    frame[headerLength + index] = payload[index]! ^ mask[index % 4]!;
  }
  return frame;
}
