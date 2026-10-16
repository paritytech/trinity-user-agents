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
      } else if (op.op === "messages") {
        send({
          kind: "answer",
          op: "messages",
          reqId: op.reqId,
          ok: { messages: [text("n0", "product", "first")], next: null },
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
    assert.deepEqual(await chat.page("notes", { pageSize: 20 }), {
      messages: [text("n0", "product", "first")],
      next: null,
    });
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
      {
        op: "messages",
        roomId: "notes",
        before: true,
        cursor: null,
        pageSize: 20,
        reqId: 2,
      },
    ]);
  } finally {
    chat.close();
    host.close();
  }
});

test("the helper sends the message actions of a room that the worker keeps", async () => {
  const host = await fakeHost(
    {
      kind: "snapshot",
      productId: "verbosity.dot",
      rooms: [{ roomId: "dm-7", name: "bob" }],
      messages: [],
    },
    (op, send) => {
      const ok =
        op.op === "participants"
          ? { type: "Resolved", participants: [] }
          : op.op === "search"
            ? { hits: [], next: null }
            : { type: "Acknowledged" };
      if (op.op === "delete" && op.messageId === "i9")
        send({
          kind: "answer",
          op: op.op,
          reqId: op.reqId,
          error: { tag: "Rejected", reason: "no message 9 in this chat" },
        });
      else send({ kind: "answer", op: op.op, reqId: op.reqId, ok });
    },
  );
  const chat = await ChatSocket.connect(host.port);
  try {
    assert.deepEqual(await chat.edit("dm-7", "i3", "hi, edited"), {
      type: "Acknowledged",
    });
    await chat.react("dm-7", "i3", "👍");
    await chat.react("dm-7", "i3", "👍", { placed: false });
    await chat.seen("dm-7", "i3");
    await chat.delete("dm-7", "i3");
    await assert.rejects(chat.delete("dm-7", "i9"), /delete refused: no message 9/);
    assert.deepEqual(
      await chat.participants("dm-7", { type: "ByIds", participantIds: ["c7"] }),
      { type: "Resolved", participants: [] },
    );
    assert.deepEqual(await chat.search("dm-7", "hi"), { hits: [], next: null });
    assert.deepEqual(
      host.received.map(({ reqId: _reqId, ...op }) => op),
      [
        { op: "edit", roomId: "dm-7", messageId: "i3", text: "hi, edited" },
        { op: "react", roomId: "dm-7", messageId: "i3", emoji: "👍", placed: true },
        { op: "react", roomId: "dm-7", messageId: "i3", emoji: "👍", placed: false },
        { op: "seen", roomId: "dm-7", messageId: "i3" },
        { op: "delete", roomId: "dm-7", messageId: "i3" },
        { op: "delete", roomId: "dm-7", messageId: "i9" },
        {
          op: "participants",
          roomId: "dm-7",
          query: { type: "ByIds", participantIds: ["c7"] },
        },
        { op: "search", roomId: "dm-7", text: "hi", cursor: null },
      ],
    );
  } finally {
    chat.close();
    host.close();
  }
});

// The live checks need a built host and guests:
//   cargo build -p truapi-host-cli
//   cargo build --manifest-path rust/guests/Cargo.toml --target wasm32-unknown-unknown --release
//   TRUAPI_HOST_BIN=target/debug/truapi-host \
//   CHAT_ACTIONS_WASM=rust/guests/target/wasm32-unknown-unknown/release/chat_actions_guest.wasm \
//   CHAT_SOURCE_WASM=rust/guests/target/wasm32-unknown-unknown/release/chat_source_guest.wasm \
//   node --test scripts/lib/cli-chat-socket.test.mjs
const hostBin = process.env.TRUAPI_HOST_BIN;
const guestWasm = process.env.CHAT_ACTIONS_WASM;
const sourceWasm = process.env.CHAT_SOURCE_WASM;

/**
 * Run `truapi-host dev` with the wasm worker `wasm` as the Worker of
 * `productId`, wait for the log line `ready`, and give the port and the
 * output. `stop()` ends the host.
 */
async function devHost(wasm, productId, ready) {
  const base = mkdtempSync(join(tmpdir(), "cli-chat-socket-"));
  const port = 20_000 + Math.floor(Math.random() * 20_000);
  const child = spawn(
    hostBin,
    [
      "dev",
      "--port",
      String(port),
      "--product-id",
      productId,
      "--worker-wasm",
      wasm,
      "--base-path",
      base,
      "--mnemonic",
      "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  const host = {
    port,
    output: "",
    stop() {
      child.kill("SIGTERM");
      rmSync(base, { recursive: true, force: true });
    },
  };
  child.stdout.on("data", (chunk) => (host.output += chunk));
  child.stderr.on("data", (chunk) => (host.output += chunk));
  try {
    await waitFor(() => host.output.includes(ready), {
      timeoutMs: 120_000,
      message: () => `the worker did not start:\n${host.output}`,
    });
  } catch (error) {
    host.stop();
    throw error;
  }
  return host;
}
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
    const host = await devHost(
      guestWasm,
      "chat-actions.dot",
      "[worker] room actions ready",
    );
    let chat;
    try {
      chat = await ChatSocket.connect(host.port);
      const menu = await chat.waitForMessage("actions", "Press a button");
      await chat.post("actions", "hello");
      await chat.waitForMessage("actions", "echo: hello");
      await chat.pressNewest("actions", "yes");
      await chat.waitForMessage("actions", `pressed yes on ${menu.messageId}`);
      await chat.command("actions", "ping", "a b");
      await chat.waitForMessage("actions", "command /ping a b");
      assert.match(host.output, /\[worker\] command \/ping a b/);
    } finally {
      chat?.close();
      host.stop();
    }
  },
);

test(
  "a person reads and writes a room that a wasm worker keeps",
  {
    skip:
      hostBin && sourceWasm
        ? false
        : "set TRUAPI_HOST_BIN and CHAT_SOURCE_WASM",
  },
  async () => {
    assert.ok(existsSync(hostBin) && existsSync(sourceWasm));
    const host = await devHost(
      sourceWasm,
      "chat-source.dot",
      "[worker] room notes ready",
    );
    let chat;
    try {
      chat = await ChatSocket.connect(host.port);
      await chat.waitForRoom("notes");
      const first = await chat.page("notes");
      const posted = await chat.post("notes", "hello");
      const second = await chat.page("notes");

      assert.deepEqual(first.messages.map(messageText), ["Welcome card"]);
      assert.deepEqual(
        [posted.type, posted.message.messageId, messageText(posted.message)],
        ["MessageCreated", "n1", "hello"],
      );
      assert.deepEqual(second.messages.map(messageText), [
        "Welcome card",
        "hello",
        "noted: hello",
      ]);
      assert.match(host.output, /\[worker\] stored n1: hello/);
    } finally {
      chat?.close();
      host.stop();
    }
  },
);
