import { describe, expect, test } from "bun:test";
import {
  ChatDiagnostics,
  describeEvent,
  sanitizeReason,
  shortHex,
  shortId,
} from "./diagnostics.js";

describe("chat diagnostics", () => {
  test("keeps at most its limit, oldest dropped first", () => {
    const diagnostics = new ChatDiagnostics(3, () => 7);
    for (let index = 0; index < 5; index++)
      diagnostics.record({
        type: "set-aside",
        stream: "requests",
        reason: `r${index}`,
      });
    expect(diagnostics.list().map((event) => [event.seq, event.at])).toEqual([
      [3, 7],
      [4, 7],
      [5, 7],
    ]);
    diagnostics.clear();
    expect(diagnostics.list()).toEqual([]);
  });

  test("shortens hex runs and long text in reasons", () => {
    const key = `0x${"ab".repeat(32)}`;
    expect(sanitizeReason(`bad ${key} and ${"cd".repeat(20)}`)).toBe(
      "bad 0xabab…abab and 0xcdcd…cdcd",
    );
    expect(sanitizeReason("x".repeat(500)).length).toBe(201);
    expect(shortHex(key)).toBe("0xabab…abab");
    expect(shortId("123e4567-e89b-12d3-a456-426614174000")).toBe("123e4567…");
  });

  test("describes a received session request by its kinds", () => {
    expect(
      describeEvent({
        type: "session",
        peer: "0xee17…ad3b",
        data: "request",
        requestId: "1a2b3c4d…",
        kinds: [17, 1, null],
        duplicates: 1,
      }),
    ).toBe(
      "request 1a2b3c4d… from 0xee17…ad3b · kinds [17, 1, ?] · 1 already seen",
    );
  });
});
