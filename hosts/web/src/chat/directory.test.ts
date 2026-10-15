import { describe, expect, test } from "bun:test";
import { hexToBytes } from "@parity/truapi/scale";
import { bytesToHex } from "@parity/truapi/scale";
import { concat, text } from "./codec.js";
import {
  accountNamesKey,
  chatKeyOf,
  decodeAccountNames,
  decodePeopleConsumer,
  labelOf,
  liteLabelOwnerKey,
  resolveChatIdentity,
  usernameOwnerKey,
} from "./directory.js";

describe("chain directory", () => {
  test("map keys match keys read from Paseo Next v2 Asset Hub", () => {
    // Both keys were listed with state_getKeysPaged on 2026-10-15.
    expect(liteLabelOwnerKey("sweepipcnqfvtu.11")).toBe(
      "0x57ca9018d648b40a8286a471e82c3808fd6d83e29558d6c9f6558f7b19f9b5f10006281d2057aa6cbabc37ac2c91178c4473776565706970636e71667674752e3131",
    );
    expect(
      accountNamesKey(
        hexToBytes(
          "0xd65a8849b41f0a738e73966599a04c7fedf3d56f5b4a76b3731097da3bfc9565",
        ),
      ),
    ).toBe(
      "0x57ca9018d648b40a8286a471e82c3808de072516e985ec03dec70088d3f16ce3000bd8329cee471b1b8f6d7d8a8682ffd65a8849b41f0a738e73966599a04c7fedf3d56f5b4a76b3731097da3bfc9565",
    );
  });

  test("decodes a stored AccountNames value with a lite label and its chat key", () => {
    // The value stored under the AccountNames key above, read the same day.
    const names = decodeAccountNames(
      hexToBytes(
        "0x01307471657865756169712e303101008af389709e82f486d44945ad1a682e102948887ae3f97429376841fbe7e51310000000000000000000000000000000000000000000000000000000000000000000",
      ),
    );
    expect(labelOf(names)).toBe("tqexeuaiq.01");
    expect(names.full).toBeNull();
    expect(chatKeyOf(names)).toEqual(
      hexToBytes(
        "0x8af389709e82f486d44945ad1a682e102948887ae3f97429376841fbe7e51310",
      ),
    );
  });
});

describe("People chain records", () => {
  test("the username owner key matches a key read from Paseo Next v2 People", () => {
    // Listed with state_getKeysPaged on 2026-10-15.
    expect(usernameOwnerKey("sweepipcnqfvtu.11")).toBe(
      "0x2111e0df19de9563b58301e5f7e00743099c711e270af23bb9a4e46759a9761d0006281d2057aa6cbabc37ac2c91178c4473776565706970636e71667674752e3131",
    );
  });

  // The layout of a live Lite consumer, with a synthetic key and username.
  const key = new Uint8Array(32).fill(0x5a);
  const consumer = (keyType: number) =>
    concat(
      Uint8Array.of(keyType),
      key,
      new Uint8Array(32),
      Uint8Array.of(0),
      text("fixture.07"),
      Uint8Array.of(0),
    );

  test("reads the chat key of a Consumers identifier key", () => {
    expect(decodePeopleConsumer(consumer(0))).toEqual({
      liteUsername: "fixture.07",
      fullUsername: null,
      chatKey: key,
    });
    expect(decodePeopleConsumer(consumer(1)).chatKey).toBeNull();
  });

  const names = (chatKey: Uint8Array | null) => ({
    lite: { label: "fixture.07", chatKey },
    full: null,
  });
  const people = (chatKey: Uint8Array | null) => ({
    liteUsername: "fixture.07",
    fullUsername: null,
    chatKey,
  });
  const other = new Uint8Array(32).fill(0x33);

  test("takes the key from whichever chain names one", () => {
    expect(resolveChatIdentity(null, null)).toBeNull();
    expect(resolveChatIdentity(null, people(key))).toEqual({
      name: "fixture.07",
      chatKey: key,
      conflict: false,
      from: ["people"],
    });
    expect(resolveChatIdentity(names(key), null)?.chatKey).toEqual(key);
    expect(resolveChatIdentity(names(null), people(key))?.chatKey).toEqual(key);
    expect(resolveChatIdentity(names(key), people(key))?.conflict).toBe(false);
  });

  test("uses neither key when the chains name different ones", () => {
    expect(resolveChatIdentity(names(key), people(other))).toEqual({
      name: "fixture.07",
      chatKey: null,
      conflict: true,
      from: ["asset hub", "people"],
    });
    expect(bytesToHex(key)).not.toBe(bytesToHex(other));
  });
});
