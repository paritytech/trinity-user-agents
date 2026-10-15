// The core loads its ring-VRF module on first use. Under Node that module is a
// file on disk, which Node's fetch cannot read, so the core must not need fetch
// for it. Uses the testing bundle, the one that exposes `ringVrfMember`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  initSync,
  ringVrfMember,
} from "../../dist/wasm/testing/truapi_server.js";

test("the ring-VRF module loads from disk with fetch unavailable", async () => {
  initSync({
    module: readFileSync(
      new URL("../../dist/wasm/testing/truapi_server_bg.wasm", import.meta.url),
    ),
  });
  const realFetch = globalThis.fetch;
  globalThis.fetch = () => {
    throw new Error(
      "fetch must not be used to load the ring-VRF module under Node",
    );
  };
  try {
    const member = await ringVrfMember(new Uint8Array(32).fill(7));
    assert.ok(member.length > 0);
  } finally {
    globalThis.fetch = realFetch;
  }
});
