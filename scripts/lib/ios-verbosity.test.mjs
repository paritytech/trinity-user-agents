// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { readIdentity } from "./ios-verbosity.mjs";

const NETWORK = "paseo-next-v2";
const WORDS = "one two three four five six seven eight nine ten eleven twelve";

function session(base, network, name, accounts) {
  const directory = join(base, "v2", network, `${name}_signing_host`);
  mkdirSync(directory, { recursive: true });
  writeFileSync(
    join(directory, "accounts.json"),
    JSON.stringify({ version: 1, accounts }),
  );
}

function account(overrides) {
  return {
    name: "auto-1",
    network: NETWORK,
    mnemonic: WORDS,
    lite_username: "verbosityiosa.72",
    attested: true,
    ...overrides,
  };
}

test("an attested account of the network is the identity of the user", () => {
  const base = mkdtempSync(join(tmpdir(), "ios-verbosity-"));
  session(base, NETWORK, "verbosityiosa.72", [account()]);

  assert.deepEqual(readIdentity(base, NETWORK), {
    username: "verbosityiosa.72",
    mnemonic: WORDS,
  });
});

// An account whose username the backend never registered would give the app
// a wallet with no name on chain, which is the gap the harness closes.
test("an account that is not attested is no identity", () => {
  const base = mkdtempSync(join(tmpdir(), "ios-verbosity-"));
  session(base, NETWORK, "verbosityiosa.72", [account({ attested: false })]);

  assert.equal(readIdentity(base, NETWORK), undefined);
});

// The app of one network cannot find a name that another network registered.
test("an account of another network is no identity", () => {
  const base = mkdtempSync(join(tmpdir(), "ios-verbosity-"));
  session(base, "previewnet", "verbosityiosa.3", [
    account({ network: "previewnet", lite_username: "verbosityiosa.3" }),
  ]);

  assert.equal(readIdentity(base, NETWORK), undefined);
  assert.equal(readIdentity(join(base, "missing"), NETWORK), undefined);
});
