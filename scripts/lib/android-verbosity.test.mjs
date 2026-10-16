// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { test } from "node:test";

import { chatIdHex } from "./android-emulator.mjs";
import {
  EMULATORS,
  USERS,
  handInstalledWorker,
  productRoomId,
  workerLogLines,
} from "./android-verbosity.mjs";

test("the worker installed by hand lies in the folder of the product, and a file URL names it", () => {
  assert.deepEqual(handInstalledWorker("io.parity.polkadotapp.debug", "verbosity.dot"), {
    directory: "files/Products/verbosity.dot/ChatExtension",
    entry: "files/Products/verbosity.dot/ChatExtension/index.wasm",
    manifest: "files/Products/verbosity.dot/ChatExtension/manifest.json",
    url: "file:///data/user/0/io.parity.polkadotapp.debug/files/Products/verbosity.dot/ChatExtension/index.wasm",
  });
});

test("the room id of a product room comes from its chat id", () => {
  const chat = chatIdHex("ProductBot_verbosity.dot", "dm-7f3a");

  assert.equal(productRoomId(chat, "verbosity.dot"), "dm-7f3a");
  assert.equal(productRoomId(chat, "other.dot"), undefined);
  assert.equal(productRoomId(Buffer.from("a direct chat").toString("hex"), "verbosity.dot"), undefined);
});

test("the worker log is the wasm-worker lines of the core log", () => {
  const log = [
    "10-15 23:01:02.345  1234  1250 D truapi.core: wasm-worker: engine started",
    "10-15 23:01:02.400  1234  1250 D truapi.core: chain: connected",
    "10-15 23:01:03.000  1234  1251 I truapi.e2e: product registered id=verbosity.dot",
    "10-15 23:01:04.500  1234  1250 D truapi.core: wasm-worker: request sent to @b",
  ].join("\n");

  assert.deepEqual(workerLogLines(log), [
    "23:01:02.345 engine started",
    "23:01:04.500 request sent to @b",
  ]);
});

test("each user has an isolated AVD on its own console port", () => {
  const ports = USERS.map((user) => EMULATORS[user].port);
  const avds = USERS.map((user) => EMULATORS[user].avd);

  assert.equal(new Set(ports).size, USERS.length);
  assert.equal(new Set(avds).size, USERS.length);
  for (const port of ports) assert.equal(port % 2, 0, "an emulator console port is even");
});
