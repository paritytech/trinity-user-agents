// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  formatSigningHostExit,
  sanitizeSigningHostOutput,
  signingHostPairArgs,
  type SigningHostCliConfig,
} from "./signing-host-cli";

const config: SigningHostCliConfig = {
  binary: "/repo/target/debug/truapi-host",
  cwd: "/repo",
  basePath: "/repo/.e2e-dotli/signing-host",
  network: "paseo-next-v2",
  session: "dotlitest",
};

describe("signing-host CLI pairing", () => {
  it("builds an isolated auto-accept pair command", () => {
    assert.deepEqual(
      signingHostPairArgs(config, "polkadotapp://pair?handshake=01"),
      [
        "signing-host",
        "--network",
        "paseo-next-v2",
        "--base-path",
        "/repo/.e2e-dotli/signing-host",
        "--auto-accept",
        "--session",
        "dotlitest",
        "exec",
        "/pair polkadotapp://pair?handshake=01",
      ],
    );
  });

  it("omits managed-account flags for an explicit mnemonic", () => {
    assert.ok(
      !signingHostPairArgs(
        { ...config, session: undefined },
        "polkadotapp://pair?handshake=01",
      ).includes("--session"),
    );
  });

  it("redacts pairing deeplinks from logs and failures", () => {
    const output =
      "pairing polkadotapp://pair?handshake=secret\nrequest accepted";
    assert.equal(
      sanitizeSigningHostOutput(output),
      "pairing <pairing deeplink>\nrequest accepted",
    );
    assert.ok(
      formatSigningHostExit({ code: 1, signal: null }, output).includes(
        "pairing <pairing deeplink>",
      ),
    );
    assert.ok(
      !formatSigningHostExit({ code: 1, signal: null }, output).includes(
        "secret",
      ),
    );
  });
});
