import { describe, expect, test } from "bun:test";
import type { HexString } from "@parity/truapi/scale";
import { extensionVersions } from "../../../src/app/callbacks.js";

const genesis = (byte: string) => `0x${byte.repeat(32)}` as HexString;

describe("the chains the core builds V5 transactions for", () => {
  test("are People and Asset Hub, on pipeline 1, and no other chain", () => {
    expect(
      extensionVersions({
        genesis: {
          relay: genesis("01"),
          people: genesis("02"),
          assetHub: genesis("03"),
          bulletin: genesis("04"),
        },
      }),
    ).toEqual({
      chains: [
        { genesisHash: genesis("02"), extensionVersion: 1 },
        { genesisHash: genesis("03"), extensionVersion: 1 },
      ],
    });
  });
});
