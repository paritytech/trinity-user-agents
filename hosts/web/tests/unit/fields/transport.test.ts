import { describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import {
  FIELD_TOPIC,
  openFieldTransport,
  readStatement,
} from "../../../src/fields/transport.js";
import { encodeSigned } from "../../../bridges/primitives/statement.js";
import { toHex, utf8 } from "../../../bridges/primitives/bytes.js";
import type { Listing } from "../../../bridges/fields/protocol.js";

describe("field transport initialization cancellation", () => {
  test("rejects an already-ended session without touching the runtime or store", async () => {
    const controller = new AbortController();
    controller.abort();
    let touched = false;
    await expect(
      openFieldTransport({
        runtime: {
          createProvider: async () => {
            touched = true;
            throw new Error("unexpected runtime call");
          },
        },
        product: "localhost:5183",
        signal: controller.signal,
        network: { kind: "simulated", name: "test" },
        store: async () => {
          touched = true;
          throw new Error("unexpected store call");
        },
        allow: async () => {
          touched = true;
        },
        receive: () => {
          touched = true;
        },
      }),
    ).rejects.toMatchObject({ code: "session" });
    expect(touched).toBe(false);
  });

  test("does not wait for a stalled provider after cancellation or open a store later", async () => {
    const controller = new AbortController();
    let storeOpened = false;
    let rejectProvider!: (error: Error) => void;
    const pending = openFieldTransport({
      runtime: {
        createProvider: () =>
          new Promise<never>((_resolve, reject) => {
            rejectProvider = reject;
          }),
      },
      product: "localhost:5183",
      signal: controller.signal,
      network: { kind: "simulated", name: "test" },
      store: async () => {
        storeOpened = true;
        throw new Error("unexpected store call");
      },
      allow: async () => {},
      receive: () => {},
    });
    controller.abort();
    await expect(pending).rejects.toMatchObject({ code: "session" });
    rejectProvider(new Error("late provider failure"));
    await Promise.resolve();
    expect(storeOpened).toBe(false);
  });
});

describe("received demo statements", () => {
  const now = 1_800_000_000_000;
  const secret = sr25519.secretFromSeed(new Uint8Array(32).fill(7));
  const signer = sr25519.getPublicKey(secret);
  const listing: Listing = {
    v: 1,
    type: "listing",
    id: crypto.randomUUID(),
    item: "Blue notebook",
    seller: toHex(signer),
    key: `0x${"9".repeat(64)}`,
  };
  /** A statement signed by the test key, expiring `seconds` after `now`. */
  const signed = (
    seconds: number,
    options: { data?: unknown; topic?: Uint8Array } = {},
  ) =>
    encodeSigned(
      {
        expiry: BigInt(Math.floor(now / 1000) + seconds) << 32n,
        channel: new Uint8Array(32),
        topics: [options.topic ?? FIELD_TOPIC],
        data: utf8(JSON.stringify(options.data ?? listing)),
      },
      signer,
      (payload) => sr25519.sign(secret, payload),
    );

  test("reads a signed demo message and when it expires", () => {
    expect(readStatement(signed(3600), now)?.received).toEqual({
      sender: toHex(signer),
      message: listing,
      expires: now + 3_600_000,
    });
  });

  test("drops what has expired, expires too far ahead, is off topic, misnames its seller or is tampered", () => {
    const tampered = signed(3600);
    tampered[tampered.length - 1] ^= 1;
    expect(
      [
        signed(0),
        signed(-60),
        signed(3 * 3600),
        signed(3600, { topic: new Uint8Array(32) }),
        signed(3600, { data: { ...listing, seller: `0x${"1".repeat(64)}` } }),
        tampered,
      ].map((encoded) => readStatement(encoded, now)),
    ).toEqual([null, null, null, null, null, null]);
  });
});
