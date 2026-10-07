// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect } from "bun:test";
import { explainProductError, isProbableSsoTimeout } from "../src/errors.js";

describe("isProbableSsoTimeout", () => {
  it("As a host embedder, the bare TxError the 180s SSO timeout produces is recognized as one", () => {
    // Then
    expect(
      isProbableSsoTimeout({ isSdkError: true, source: "tx", name: "TxError" }),
    ).toBe(true);
  });

  it("As a host embedder, a TxError carrying a real message is never flagged as an SSO timeout", () => {
    // Then
    expect(
      isProbableSsoTimeout({
        isSdkError: true,
        source: "tx",
        name: "TxError",
        message: "insufficient funds",
      }),
    ).toBe(false);
  });

  it("As a host embedder, unrelated errors are never flagged as SSO timeouts", () => {
    // Then
    expect(isProbableSsoTimeout(new Error("boom"))).toBe(false);
    expect(isProbableSsoTimeout(undefined)).toBe(false);
    expect(
      isProbableSsoTimeout({ isSdkError: true, source: "rpc", name: "X" }),
    ).toBe(false);
  });
});

describe("explainProductError", () => {
  it("As a CLI user, I hit the probable SSO timeout and get phone-facing guidance", () => {
    // When
    const explained = explainProductError({
      isSdkError: true,
      source: "tx",
      name: "TxError",
    });

    // Then
    expect(explained).toMatch(/no response from your phone/i);
  });

  it("As a CLI user, the host stays silent for errors it cannot improve on", () => {
    // Then
    expect(explainProductError(new Error("boom"))).toBeUndefined();
  });
});
