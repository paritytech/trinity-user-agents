import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { encodeSigned } from "../../../../bridges/primitives/statement.js";
import { compact, concat, u64 } from "../../../../bridges/primitives/codec.js";
import {
  readFilter,
  readPage,
  readStatement,
  readSubmitAnswer,
} from "../../../../tools/statements/decode.js";

const SIGNER = new Uint8Array(32).fill(7);
const TOPIC = new Uint8Array(32).fill(1);
const CHANNEL = new Uint8Array(32).fill(2);

describe("reading statements", () => {
  test("reads a signed statement's metadata without verifying it, and keeps its data apart", () => {
    const expiry = (1_900_000_000n << 32n) | 5n;
    const encoded = encodeSigned(
      { expiry, channel: CHANNEL, topics: [TOPIC], data: new Uint8Array(10) },
      SIGNER,
      () => new Uint8Array(64),
    );
    const { meta, data } = readStatement(encoded);
    expect(data).toEqual(new Uint8Array(10));
    expect({ ...meta, hash: meta.hash.length }).toEqual({
      hash: 66,
      encodedBytes: encoded.length,
      proof: "Sr25519",
      signer: bytesToHex(SIGNER),
      expiry: expiry.toString(),
      expiresAt: 1_900_000_000,
      priority: 5,
      channel: bytesToHex(CHANNEL),
      topics: [bytesToHex(TOPIC)],
      decryptionKey: false,
      dataBytes: 10,
    });
  });

  test("reads an ECDSA proof and an expiry that never lapses", () => {
    const encoded = concat(
      compact(2),
      Uint8Array.of(0, 2),
      new Uint8Array(65),
      new Uint8Array(33).fill(3),
      Uint8Array.of(2),
      u64((0xffff_ffffn << 32n) | 9n),
    );
    const { meta } = readStatement(encoded);
    expect(meta.proof).toBe("Ecdsa");
    expect(meta.signer).toBe(bytesToHex(new Uint8Array(33).fill(3)));
    expect(meta.expiresAt).toBeNull();
    expect(meta.undecoded).toBeUndefined();
  });

  test("keeps what came before a field it does not know, and says why it stopped", () => {
    const encoded = concat(
      compact(2),
      Uint8Array.of(3),
      CHANNEL,
      Uint8Array.of(9, 1),
    );
    const { meta } = readStatement(encoded);
    expect(meta.channel).toBe(bytesToHex(CHANNEL));
    expect(meta.undecoded).toBe("a field tagged 9");
  });
});

describe("reading the store's answers", () => {
  test("keeps the reason, its numbers by their own names, and the answer as it came", () => {
    expect(
      readSubmitAnswer({
        status: "rejected",
        reason: "dataTooLarge",
        submitted_size: 600_000,
        available_size: 12_000,
      }),
    ).toEqual({
      status: "rejected",
      reason: "dataTooLarge",
      fields: { submitted_size: 600_000, available_size: 12_000 },
      shape:
        '{"status":"rejected","reason":"dataTooLarge","submitted_size":600000,"available_size":12000}',
    });
    expect(readSubmitAnswer("ok").status).toBe("unrecognised");
    expect(readSubmitAnswer({ status: "something new" }).status).toBe(
      "unrecognised",
    );
  });

  test("reads filters and pages, and keeps a shape it does not know", () => {
    expect(readFilter([{ matchAny: ["0xAB"] }])).toEqual({
      kind: "matchAny",
      topics: ["0xab"],
    });
    expect(readFilter(["any"])).toEqual({ kind: "any" });
    expect(readFilter([{ other: 1 }]).kind).toBe("unrecognised");
    expect(
      readPage({
        event: "newStatements",
        data: { statements: ["0x00"], remaining: 3 },
      }),
    ).toEqual({ kind: "statements", statements: ["0x00"], remaining: 3 });
    expect(
      readPage({ event: "newStatements", data: { statements: [] } }),
    ).toEqual({
      kind: "statements",
      statements: [],
    });
    expect(readPage({ event: "dropped" }).kind).toBe("unrecognised");
  });
});
