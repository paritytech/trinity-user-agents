// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
import { describe, expect, it } from "bun:test";

import { channelEvictionVictim } from "./session.js";

const entry = (lastSeen: number, codecOk: boolean) => ({ lastSeen, codecOk });

describe("channelEvictionVictim", () => {
  it("evicts the least recently seen when everything is trusted", () => {
    const channels = new Map([
      ["a", entry(3, true)],
      ["b", entry(1, true)],
      ["c", entry(2, true)],
    ]);
    expect(channelEvictionVictim(channels)).toBe("b");
  });

  it("spares a distrusted channel while a trusted one can go instead", () => {
    // "b" is the oldest, so a plain LRU would evict it - and a re-registration
    // would then rebuild its record clean, clearing the drift chip. That is the
    // laundering this ordering exists to prevent.
    const channels = new Map([
      ["a", entry(3, true)],
      ["b", entry(1, false)],
      ["c", entry(2, true)],
    ]);
    expect(channelEvictionVictim(channels)).toBe("c");
  });

  it("spares the distrusted channel even when it is the most recently seen", () => {
    const channels = new Map([
      ["a", entry(9, false)],
      ["b", entry(1, true)],
    ]);
    expect(channelEvictionVictim(channels)).toBe("b");
  });

  it("falls back to the oldest once every channel is distrusted", () => {
    // Nothing left to spare it in favour of. The board already reports drift on
    // every channel here, so rebuilding one verdict hides nothing.
    const channels = new Map([
      ["a", entry(2, false)],
      ["b", entry(1, false)],
    ]);
    expect(channelEvictionVictim(channels)).toBe("b");
  });

  it("has no victim for an empty registry", () => {
    expect(channelEvictionVictim(new Map())).toBeUndefined();
  });
});
