// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT

// The prompt-content contract for the reviews where wording is
// safety-relevant: whether an approval is final at the terminal or verified
// again on the phone must match where signing actually happens.

import { describe, it, expect } from "bun:test";
import { describeReview } from "../src/reviews.js";

describe("describeReview", () => {
  it("As a CLI user, a Bulletin publish prompt tells me approval is final and never promises a phone checkpoint", () => {
    // Given
    // Bulletin writes sign in-core with the allowance key. Measured with the
    // phone off: phone-dependent operations time out, Bulletin writes
    // succeed. The prompt must not claim otherwise.
    const request = describeReview({
      tag: "PreimageSubmit",
      value: { size: 614400n },
    });

    // Then
    expect(request.phoneVerifies).toBe(false);
    expect(request.phoneNote).toMatch(/final/i);
    expect(request.phoneNote).not.toMatch(/phone/i);
    expect(request.details.join(" ")).toContain("614400 bytes");
  });

  it("As a CLI user, a raw-message signing prompt still defers to the phone", () => {
    // Given
    const request = describeReview({
      tag: "SignRaw",
      value: {
        tag: "Product",
        value: {
          request: {
            account: {
              dotNsIdentifier: "test.dot",
              derivationIndex: { tag: "Index", value: 0 },
            },
            payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
          },
          watermarked: false,
        },
      },
    });

    // Then
    expect(request.phoneVerifies).toBe(true);
    expect(request.phoneNote).toBeUndefined();
    expect(request.details.join(" ")).toContain("raw binary data (4 bytes)");
    // Unwatermarked raw bytes are the dangerous signing shape. The prompt
    // must say so.
    expect(request.details.join(" ")).toContain("no <Bytes> envelope");
  });

  it("As a CLI user, a transaction prompt names the product that asked, the account and the chain", () => {
    // Given
    const genesisHash = `0x${"ab".repeat(32)}` as const;
    const request = describeReview(
      {
        tag: "CreateTransaction",
        value: {
          tag: "Product",
          value: {
            callingProductId: "caller.dot",
            payload: {
              signer: {
                dotNsIdentifier: "test.dot",
                derivationIndex: { tag: "Index", value: 0 },
              },
              genesisHash,
              callData: "0x00001070726f62",
              extensions: [],
              txExtVersion: 0,
              contacts: [],
            },
          },
        },
      },
      {
        endpoints: {
          [genesisHash]: {
            rpc: "wss://example.invalid",
            name: "Paseo Asset Hub",
          },
        },
      },
    );

    // Then
    expect(request.title).toBe("Submit a transaction on Paseo Asset Hub");
    expect(request.details).toEqual([
      "product: caller.dot",
      "account: test.dot (derivation #0)",
      "call data: 7 bytes (not decodable here)",
    ]);
    expect(request.phoneVerifies).toBe(true);
  });

  it("As a CLI user, a signing prompt without a named caller shows no product line", () => {
    // Given
    const request = describeReview({
      tag: "SignPayload",
      value: {
        tag: "Product",
        value: {
          request: {
            account: {
              dotNsIdentifier: "test.dot",
              derivationIndex: { tag: "Index", value: 2 },
            },
            payload: {
              blockHash: "0x00",
              blockNumber: "0x00",
              era: "0x00",
              genesisHash: "0x00",
              method: "0x00",
              nonce: "0x00",
              specVersion: "0x00",
              tip: "0x00",
              transactionVersion: "0x00",
              signedExtensions: [],
              version: 4,
            },
          },
        },
      },
    });

    // Then
    expect(request.details).toEqual(["account: test.dot (derivation #2)"]);
  });
});
