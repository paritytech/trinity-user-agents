// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Act as the person in the chat of a `truapi-host dev` host, with no browser.
//
// The helper speaks the protocol of the `/chat` page on its socket
// `/chat/ws`: it keeps the rooms and the messages the host sends, and sends
// the operations the page sends when a person types a message, presses a
// button of a message, runs a command, reads a page of a room that the
// worker keeps (`productSource`), or edits, deletes, reacts to, sees and
// searches the messages of such a room. A test of a worker (a wasm worker that
// `--worker-wasm` runs, or the JS worker of `--worker-bundle`) uses it to see
// what the worker draws and to answer it.
//
//   const chat = await ChatSocket.connect("http://127.0.0.1:9955/chat");
//   await chat.waitForMessage("actions", (m) => messageText(m) === "Press a button");
//   await chat.press("actions", menu.messageId, "yes");
//   await chat.post("actions", "hello");
//   chat.close();
//
// From a terminal, with the same host:
//
//   node scripts/lib/cli-chat-socket.mjs 9955 rooms
//   node scripts/lib/cli-chat-socket.mjs 9955 messages actions
//   node scripts/lib/cli-chat-socket.mjs 9955 page notes
//   node scripts/lib/cli-chat-socket.mjs 9955 post actions "hello"
//   node scripts/lib/cli-chat-socket.mjs 9955 press actions m1 yes
//   node scripts/lib/cli-chat-socket.mjs 9955 command actions ping "some text"
//   node scripts/lib/cli-chat-socket.mjs 9955 watch

import { pathToFileURL } from "node:url";
import { waitFor } from "./process.mjs";

/** Participant id of the person in a room the host keeps. */
export const USER = "user";

const DEFAULT_TIMEOUT_MS = 30_000;

/**
 * The socket URL of a chat page. Takes a port, `host:port`, the page URL
 * `http://host:port/chat`, or the socket URL itself.
 */
export function socketUrl(target) {
  const text = String(target).trim();
  if (/^\d+$/.test(text)) return `ws://127.0.0.1:${text}/chat/ws`;
  const withScheme = /^[a-z]+:\/\//i.test(text) ? text : `http://${text}`;
  const url = new URL(withScheme);
  url.protocol =
    url.protocol === "https:" || url.protocol === "wss:" ? "wss:" : "ws:";
  url.pathname = "/chat/ws";
  url.search = "";
  url.hash = "";
  return url.toString();
}

/** An empty view of the chat, before the snapshot. */
export function emptyChat() {
  return {
    productId: null,
    worker: null,
    rooms: new Map(),
    messages: new Map(),
  };
}

/**
 * Apply one frame of the host to `chat`, as the page does: the snapshot
 * replaces everything, `room` and `message` add or replace one entry, and
 * `roomDeleted` removes a room with its messages. Other frames change
 * nothing. Gives `chat`.
 */
export function applyFrame(chat, frame) {
  switch (frame.kind) {
    case "snapshot":
      chat.productId = frame.productId ?? null;
      chat.worker = frame.worker ?? null;
      chat.rooms = new Map(
        (frame.rooms ?? []).map((room) => [room.roomId, room]),
      );
      chat.messages = new Map();
      for (const entry of frame.messages ?? [])
        upsert(chat, entry.roomId, entry.message);
      break;
    case "room": {
      const { kind: _kind, ...room } = frame;
      chat.rooms.set(room.roomId, room);
      break;
    }
    case "roomDeleted":
      chat.rooms.delete(frame.roomId);
      chat.messages.delete(frame.roomId);
      break;
    case "message":
      upsert(chat, frame.roomId, frame.message);
      break;
    case "workers":
      chat.worker = { ...(chat.worker ?? {}), connected: frame.connected };
      break;
    default:
      break;
  }
  return chat;
}

function upsert(chat, roomId, message) {
  const list = chat.messages.get(roomId) ?? [];
  const index = list.findIndex(
    (known) => known.messageId === message.messageId,
  );
  if (index >= 0) list[index] = message;
  else list.push(message);
  chat.messages.set(roomId, list);
}

/** The text a person reads in `message`: the text of its body, if any. */
export function messageText(message) {
  const body = message?.body ?? {};
  if (typeof body.text === "string") return body.text;
  if (body.type === "Custom") return body.previewText ?? "";
  return "";
}

/**
 * The newest message of `messages` with a button that has the id or the
 * title `action`, as `{ message, action }`, or undefined.
 */
export function findAction(messages, action) {
  for (const message of [...messages].reverse()) {
    if (message.deleted || message.body?.type !== "Actions") continue;
    const found = message.body.actions.find(
      (candidate) =>
        candidate.actionId === action || candidate.title === action,
    );
    if (found) return { message, action: found };
  }
  return undefined;
}

/** One connection to the chat page socket of a host. */
export class ChatSocket {
  /**
   * Open the socket of `target` (see `socketUrl`) and wait for the snapshot.
   */
  static async connect(target, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    const url = socketUrl(target);
    const socket = new WebSocket(url);
    const chat = new ChatSocket(socket, url);
    await new Promise((resolveOpen, reject) => {
      const timer = setTimeout(
        () =>
          reject(
            new Error(`no chat snapshot from ${url} within ${timeoutMs} ms`),
          ),
        timeoutMs,
      );
      chat.#onSnapshot = () => {
        clearTimeout(timer);
        resolveOpen();
      };
      socket.addEventListener("error", (event) => {
        clearTimeout(timer);
        reject(
          new Error(
            `chat socket ${url} failed: ${event.message ?? event.type}`,
          ),
        );
      });
    });
    return chat;
  }

  #socket;
  #nextReqId = 1;
  /** Answers by request id. */
  #answers = new Map();
  /** Operations that the host answers at once, in the order they were sent. */
  #immediate = [];
  #onSnapshot = () => {};

  constructor(socket, url) {
    this.#socket = socket;
    this.url = url;
    this.chat = emptyChat();
    /** Every frame the host sent, oldest first. */
    this.frames = [];
    this.closed = false;
    socket.addEventListener("message", (event) => this.#receive(event.data));
    socket.addEventListener("close", () => {
      this.closed = true;
      const error = new Error(`chat socket ${url} closed`);
      for (const { reject } of this.#answers.values()) reject(error);
      for (const { reject } of this.#immediate) reject(error);
      this.#answers.clear();
      this.#immediate = [];
    });
  }

  #receive(data) {
    let frame;
    try {
      frame = JSON.parse(String(data));
    } catch {
      return;
    }
    this.frames.push(frame);
    applyFrame(this.chat, frame);
    if (frame.kind === "snapshot") this.#onSnapshot();
    if (frame.kind === "answer") {
      const pending = this.#answers.get(frame.reqId);
      this.#answers.delete(frame.reqId);
      if (!pending) return;
      if (frame.error)
        pending.reject(
          refusal(frame.op, frame.error.reason ?? frame.error.tag),
        );
      else pending.resolve(frame.ok);
      return;
    }
    if (frame.kind === "delivered" || frame.kind === "error") {
      if (frame.reqId != null && this.#answers.has(frame.reqId)) {
        const pending = this.#answers.get(frame.reqId);
        this.#answers.delete(frame.reqId);
        if (frame.kind === "error")
          pending.reject(refusal("op", frame.message));
        else pending.resolve(frame);
        return;
      }
      const pending = this.#immediate.shift();
      if (!pending) return;
      if (frame.kind === "error")
        pending.reject(refusal(pending.op, frame.message));
      else pending.resolve(frame);
    }
  }

  #send(op) {
    if (this.closed) throw new Error(`chat socket ${this.url} closed`);
    this.#socket.send(JSON.stringify(op));
  }

  /** Send `op`, which the host answers with an `answer` frame. */
  #ask(op, timeoutMs) {
    const reqId = this.#nextReqId++;
    return withTimeout(
      new Promise((resolve, reject) => {
        this.#answers.set(reqId, { resolve, reject });
        this.#send({ ...op, reqId });
      }),
      timeoutMs,
      `no answer to ${op.op} in ${op.roomId}`,
    );
  }

  /** Send `op`, which the host answers at once with `delivered` or `error`. */
  #deliver(op, timeoutMs) {
    return withTimeout(
      new Promise((resolve, reject) => {
        this.#immediate.push({ op: op.op, resolve, reject });
        this.#send(op);
      }),
      timeoutMs,
      `no reply to ${op.op} in ${op.roomId}`,
    );
  }

  /** The rooms, as the host sent them. */
  rooms() {
    return [...this.chat.rooms.values()];
  }

  /** The messages of `roomId`, oldest first. */
  messages(roomId) {
    return [...(this.chat.messages.get(roomId) ?? [])];
  }

  /**
   * Ask for one page of `roomId`, a room that the worker keeps, as the page
   * does when it opens the room or scrolls. The host asks the worker. Gives
   * the `ok` of the answer: `{ messages, next }`, oldest first.
   */
  page(
    roomId,
    {
      before = true,
      cursor = null,
      pageSize = 50,
      timeoutMs = DEFAULT_TIMEOUT_MS,
    } = {},
  ) {
    return this.#ask(
      { op: "messages", roomId, before, cursor, pageSize },
      timeoutMs,
    );
  }

  /**
   * Type `text` into `roomId` as the person. Gives the `ok` of the answer:
   * `{ type: "Posted", message, workers }` in a room the host keeps, and
   * `{ type: "MessageCreated", message }` in a room the worker keeps.
   */
  post(roomId, text, { replyTo, timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    return this.#ask(
      { op: "post", roomId, text, replyTo: replyTo ?? null },
      timeoutMs,
    );
  }

  /**
   * Edit the message `messageId` of `roomId` to `text`, as the person does
   * from the message menu. Gives the `ok` of the answer.
   */
  edit(roomId, messageId, text, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    return this.#ask({ op: "edit", roomId, messageId, text }, timeoutMs);
  }

  /** Delete the message `messageId` of `roomId`. Gives the `ok` of the answer. */
  delete(roomId, messageId, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    return this.#ask({ op: "delete", roomId, messageId }, timeoutMs);
  }

  /**
   * Place the reaction `emoji` on the message `messageId` of `roomId`, or
   * remove it with `placed: false`. Gives the `ok` of the answer.
   */
  react(
    roomId,
    messageId,
    emoji,
    { placed = true, timeoutMs = DEFAULT_TIMEOUT_MS } = {},
  ) {
    return this.#ask(
      { op: "react", roomId, messageId, emoji, placed },
      timeoutMs,
    );
  }

  /** Tell the room that the person sees `messageId`, the newest message. */
  seen(roomId, messageId, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    return this.#ask({ op: "seen", roomId, messageId }, timeoutMs);
  }

  /**
   * Ask who the participants of `roomId` are: `{ type: "Count" }`,
   * `{ type: "Page", cursor, pageSize }` or `{ type: "ByIds",
   * participantIds }`. Gives the `ok` of the answer.
   */
  participants(roomId, query, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    return this.#ask({ op: "participants", roomId, query }, timeoutMs);
  }

  /** Search `roomId` for `text`. Gives the `ok` of the answer. */
  search(
    roomId,
    text,
    { cursor = null, timeoutMs = DEFAULT_TIMEOUT_MS } = {},
  ) {
    return this.#ask({ op: "search", roomId, text, cursor }, timeoutMs);
  }

  /**
   * Press the button `actionId` of the message `messageId` in `roomId`.
   * Gives the `delivered` frame, with the number of workers.
   */
  press(roomId, messageId, actionId, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    return this.#deliver(
      { op: "trigger", roomId, messageId, actionId },
      timeoutMs,
    );
  }

  /**
   * Press the button with the id or the title `action` of the newest
   * message of `roomId` that has one. Gives the `delivered` frame.
   */
  pressNewest(roomId, action, options = {}) {
    const found = findAction(this.messages(roomId), action);
    if (!found)
      throw new Error(`no message of ${roomId} has the button ${action}`);
    return this.press(
      roomId,
      found.message.messageId,
      found.action.actionId,
      options,
    );
  }

  /** Run the command `/command payload` that `roomId` declared. */
  command(
    roomId,
    command,
    payload = "",
    { timeoutMs = DEFAULT_TIMEOUT_MS } = {},
  ) {
    return this.#deliver(
      { op: "command", roomId, command, payload },
      timeoutMs,
    );
  }

  /** Wait until `roomId` exists, and give it. */
  waitForRoom(roomId, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    return waitFor(() => this.chat.rooms.get(roomId), {
      timeoutMs,
      intervalMs: 50,
      message: () =>
        `no room ${roomId} within ${timeoutMs} ms; rooms: ${this.#roomIds()}`,
    });
  }

  /**
   * Wait until a message of `roomId` matches `match`, and give it. `match`
   * is a function of the message, a string equal to its text, or a RegExp
   * of its text.
   */
  waitForMessage(roomId, match, { timeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
    const test = matcher(match);
    return waitFor(() => this.messages(roomId).find(test), {
      timeoutMs,
      intervalMs: 50,
      message: () =>
        `no message of ${roomId} matched ${match} within ${timeoutMs} ms; texts: ${JSON.stringify(
          this.messages(roomId).map(messageText),
        )}`,
    });
  }

  #roomIds() {
    return JSON.stringify([...this.chat.rooms.keys()]);
  }

  close() {
    this.#socket.close();
  }
}

function matcher(match) {
  if (typeof match === "function") return match;
  if (match instanceof RegExp)
    return (message) => match.test(messageText(message));
  return (message) => messageText(message) === match;
}

function refusal(op, reason) {
  return new Error(`${op} refused: ${reason}`);
}

function withTimeout(promise, timeoutMs, what) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`${what} within ${timeoutMs} ms`)),
      timeoutMs,
    );
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

/** One line of a message for the terminal. */
export function describeMessage(message) {
  const buttons =
    message.body?.type === "Actions"
      ? ` [${message.body.actions.map((action) => action.actionId).join(", ")}]`
      : "";
  return `${message.messageId} ${message.origin}: ${messageText(message)}${buttons}`;
}

async function main(argv) {
  const [target, command, ...rest] = argv;
  if (!target || !command) {
    console.error(
      "usage: cli-chat-socket.mjs <port|url> rooms | messages <room> | page <room> | post <room> <text> | press <room> <message> <action> | command <room> <name> [args] | watch",
    );
    return 2;
  }
  const chat = await ChatSocket.connect(target);
  try {
    switch (command) {
      case "rooms":
        for (const room of chat.rooms())
          console.log(`${room.roomId}\t${room.name}`);
        break;
      case "messages":
        for (const message of chat.messages(rest[0]))
          console.log(describeMessage(message));
        break;
      case "page":
        for (const message of (await chat.page(rest[0])).messages)
          console.log(describeMessage(message));
        break;
      case "post":
        console.log(
          JSON.stringify(await chat.post(rest[0], rest.slice(1).join(" "))),
        );
        break;
      case "press":
        console.log(
          JSON.stringify(await chat.press(rest[0], rest[1], rest[2])),
        );
        break;
      case "command":
        console.log(
          JSON.stringify(
            await chat.command(rest[0], rest[1], rest.slice(2).join(" ")),
          ),
        );
        break;
      case "watch":
        await new Promise(() => {
          let seen = chat.frames.length;
          setInterval(() => {
            for (const frame of chat.frames.slice(seen)) {
              if (frame.kind === "message") {
                console.log(
                  `${frame.roomId} ${describeMessage(frame.message)}`,
                );
              } else if (frame.kind === "room") {
                console.log(`room ${frame.roomId} ${frame.name}`);
              }
            }
            seen = chat.frames.length;
          }, 100);
        });
        break;
      default:
        console.error(`unknown command ${command}`);
        return 2;
    }
  } catch (error) {
    console.error(error.message);
    return 1;
  } finally {
    chat.close();
  }
  return 0;
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  process.exitCode = await main(process.argv.slice(2));
}
