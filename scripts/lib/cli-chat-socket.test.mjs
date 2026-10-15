// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
  ChatSocket,
  applyFrame,
  emptyChat,
  findAction,
  messageText,
  socketUrl,
} from "./cli-chat-socket.mjs";
import { waitFor } from "./process.mjs";

const menu = {
  messageId: "m1",
  origin: "product",
  body: {
    type: "Actions",
    text: "Press a button",
    actions: [
      { actionId: "yes", title: "YES" },
      { actionId: "no", title: "NO" },
    ],
    layout: "Grid",
  },
};
const text = (messageId, origin, value) => ({
  messageId,
  origin,
  body: { type: "Text", text: value },
});

test("every form of a target names the chat socket", () => {
  assert.equal(socketUrl(9955), "ws://127.0.0.1:9955/chat/ws");
  assert.equal(socketUrl("127.0.0.1:9971"), "ws://127.0.0.1:9971/chat/ws");
  assert.equal(
    socketUrl("http://127.0.0.1:9971/chat?room=a"),
    "ws://127.0.0.1:9971/chat/ws",
  );
  assert.equal(
    socketUrl("ws://localhost:9971/chat/ws"),
    "ws://localhost:9971/chat/ws",
  );
});

test("frames build the chat as the page draws it", () => {
  const chat = emptyChat();
  applyFrame(chat, {
    kind: "snapshot",
    productId: "chat-actions.dot",
    worker: { configured: true, connected: 0 },
    rooms: [{ roomId: "actions", name: "Actions" }],
    messages: [{ roomId: "actions", message: menu }],
  });
  applyFrame(chat, { kind: "workers", connected: 1 });
  applyFrame(chat, {
    kind: "message",
    roomId: "actions",
    message: text("m2", "user", "hi"),
  });
  applyFrame(chat, {
    kind: "message",
    roomId: "actions",
    message: { ...text("m2", "user", "hi"), status: "Read" },
  });
  applyFrame(chat, { kind: "room", roomId: "dm-bob", name: "bob" });

  assert.equal(chat.productId, "chat-actions.dot");
  assert.deepEqual(chat.worker, { configured: true, connected: 1 });
  assert.deepEqual([...chat.rooms.keys()], ["actions", "dm-bob"]);
  assert.equal(chat.rooms.get("dm-bob").kind, undefined);
  assert.deepEqual(
    chat.messages
      .get("actions")
      .map((message) => [message.messageId, message.status]),
    [
      ["m1", undefined],
      ["m2", "Read"],
    ],
  );

  applyFrame(chat, { kind: "roomDeleted", roomId: "actions" });
  assert.deepEqual([...chat.rooms.keys()], ["dm-bob"]);
  assert.equal(chat.messages.get("actions"), undefined);
});

test("a button is found by its id or its title on the newest message", () => {
  const older = { ...menu, messageId: "m0" };
  const messages = [older, text("m1a", "user", "x"), menu];
  assert.equal(findAction(messages, "yes").message.messageId, "m1");
  assert.equal(findAction(messages, "NO").action.actionId, "no");
  assert.equal(findAction(messages, "maybe"), undefined);
  assert.equal(findAction([{ ...menu, deleted: true }], "yes"), undefined);
  assert.equal(messageText(menu), "Press a button");
  assert.equal(messageText(text("m2", "user", "hi")), "hi");
});

/**
 * A chat socket on a plain HTTP server, enough for text frames under 64 KiB.
 * `answer(op, send)` replies to each operation the client sends.
 */
async function fakeHost(snapshot, answer) {
  const server = createServer();
  const received = [];
  server.on("upgrade", (request, socket) => {
    const accept = createHash("sha1")
      .update(
        `${request.headers["sec-websocket-key"]}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`,
      )
      .digest("base64");
    socket.write(
      `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
    );
    const send = (value) => {
      const payload = Buffer.from(JSON.stringify(value));
      const head =
        payload.length < 126
          ? Buffer.from([0x81, payload.length])
          : Buffer.from([
              0x81,
              126,
              payload.length >> 8,
              payload.length & 0xff,
            ]);
      socket.write(Buffer.concat([head, payload]));
    };
    let buffered = Buffer.alloc(0);
    socket.on("data", (chunk) => {
      buffered = Buffer.concat([buffered, chunk]);
      while (buffered.length >= 2) {
        const opcode = buffered[0] & 0x0f;
        let length = buffered[1] & 0x7f;
        let offset = 2;
        if (length === 126) {
          length = buffered.readUInt16BE(2);
          offset = 4;
        }
        if (buffered.length < offset + 4 + length) return;
        const mask = buffered.subarray(offset, offset + 4);
        const payload = Buffer.from(
          buffered.subarray(offset + 4, offset + 4 + length),
        );
        for (let index = 0; index < payload.length; index += 1)
          payload[index] ^= mask[index % 4];
        buffered = buffered.subarray(offset + 4 + length);
        if (opcode === 0x8) {
          socket.end();
          return;
        }
        const op = JSON.parse(payload.toString());
        received.push(op);
        answer(op, send);
      }
    });
    socket.on("error", () => {});
    send(snapshot);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { port: server.address().port, received, close: () => server.close() };
}

test("the helper sends the operations of the page and matches each reply", async () => {
  const host = await fakeHost(
    {
      kind: "snapshot",
      productId: "chat-actions.dot",
      worker: { configured: true, connected: 1 },
      rooms: [{ roomId: "actions", name: "Actions" }],
      messages: [{ roomId: "actions", message: menu }],
    },
    (op, send) => {
      if (op.op === "post") {
        const posted = text("m2", "user", op.text);
        send({ kind: "message", roomId: op.roomId, message: posted });
        send({
          kind: "answer",
          op: "post",
          reqId: op.reqId,
          ok: { type: "Posted", message: posted, workers: 1 },
        });
        send({
          kind: "message",
          roomId: op.roomId,
          message: text("m3", "product", `echo: ${op.text}`),
        });
      } else if (op.op === "command" && op.command !== "ping") {
        send({
          kind: "error",
          message: `the room declares no command "${op.command}"`,
        });
      } else {
        send({ kind: "delivered", op: op.op, workers: 1 });
      }
    },
  );
  const chat = await ChatSocket.connect(`http://127.0.0.1:${host.port}/chat`);
  try {
    assert.deepEqual(chat.rooms(), [{ roomId: "actions", name: "Actions" }]);
    assert.deepEqual(chat.messages("actions"), [menu]);

    const posted = await chat.post("actions", "hello");
    assert.equal(posted.message.messageId, "m2");
    assert.equal(
      (await chat.waitForMessage("actions", "echo: hello")).messageId,
      "m3",
    );

    assert.deepEqual(await chat.pressNewest("actions", "NO"), {
      kind: "delivered",
      op: "trigger",
      workers: 1,
    });
    await assert.rejects(
      chat.command("actions", "nope"),
      /command refused: .*nope/,
    );
    assert.equal((await chat.command("actions", "ping", "a b")).op, "command");
    assert.throws(
      () => chat.pressNewest("actions", "maybe"),
      /no message of actions/,
    );
    await assert.rejects(
      chat.waitForMessage("actions", /never/, { timeoutMs: 100 }),
      /no message of actions matched/,
    );

    assert.deepEqual(host.received, [
      { op: "post", roomId: "actions", text: "hello", replyTo: null, reqId: 1 },
      { op: "trigger", roomId: "actions", messageId: "m1", actionId: "no" },
      { op: "command", roomId: "actions", command: "nope", payload: "" },
      { op: "command", roomId: "actions", command: "ping", payload: "a b" },
    ]);
  } finally {
    chat.close();
    host.close();
  }
});

// The live check needs a built host and guest:
//   cargo build -p truapi-host-cli
//   cargo build --manifest-path rust/guests/Cargo.toml --target wasm32-unknown-unknown --release
//   TRUAPI_HOST_BIN=target/debug/truapi-host \
//   CHAT_ACTIONS_WASM=rust/guests/target/wasm32-unknown-unknown/release/chat_actions_guest.wasm \
//   node --test scripts/lib/cli-chat-socket.test.mjs
const hostBin = process.env.TRUAPI_HOST_BIN;
const guestWasm = process.env.CHAT_ACTIONS_WASM;
test(
  "a person acts in the rooms of a wasm worker that truapi-host dev runs",
  {
    skip:
      hostBin && guestWasm
        ? false
        : "set TRUAPI_HOST_BIN and CHAT_ACTIONS_WASM",
  },
  async () => {
    assert.ok(existsSync(hostBin) && existsSync(guestWasm));
    const base = mkdtempSync(join(tmpdir(), "cli-chat-socket-"));
    const port = 20_000 + Math.floor(Math.random() * 20_000);
    const child = spawn(
      hostBin,
      [
        "dev",
        "--port",
        String(port),
        "--product-id",
        "chat-actions.dot",
        "--worker-wasm",
        guestWasm,
        "--base-path",
        base,
        "--mnemonic",
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
      ],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    let output = "";
    child.stdout.on("data", (chunk) => (output += chunk));
    child.stderr.on("data", (chunk) => (output += chunk));
    let chat;
    try {
      await waitFor(() => output.includes("[worker] room actions ready"), {
        timeoutMs: 120_000,
        message: () => `the worker did not start:\n${output}`,
      });
      chat = await ChatSocket.connect(port);
      const menu = await chat.waitForMessage("actions", "Press a button");
      await chat.post("actions", "hello");
      await chat.waitForMessage("actions", "echo: hello");
      await chat.pressNewest("actions", "yes");
      await chat.waitForMessage("actions", `pressed yes on ${menu.messageId}`);
      await chat.command("actions", "ping", "a b");
      await chat.waitForMessage("actions", "command /ping a b");
      assert.match(output, /\[worker\] command \/ping a b/);
    } finally {
      chat?.close();
      child.kill("SIGTERM");
      rmSync(base, { recursive: true, force: true });
    }
  },
);
