import { describe, expect, test } from "bun:test";
import { entropyContext } from "../worker/accounts";

describe("entropyContext", () => {
  // The host refuses a derive context over 32 bytes, and every session label
  // is longer than that, so a raw label would stop each session at its start.
  test("fits any label into the 32 bytes the host accepts", () => {
    const context = entropyContext("onramp:eph:fundtest:fs_oDx4NkP6ct");
    expect(context).toMatch(/^0x[0-9a-f]{64}$/);
    expect(entropyContext("onramp:eph:fundtest:fs_oDx4NkP6ct")).toBe(context);
  });
});
