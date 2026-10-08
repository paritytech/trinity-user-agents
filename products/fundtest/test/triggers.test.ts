import { expect, test } from "bun:test";
import { triggerOf } from "../worker/triggers";
import { readSaved, writeSaved } from "../worker/saved";

const cash = (whole: bigint, cents: bigint) =>
  whole * 1_000_000n + cents * 10_000n;

// Testers pick a failure case by typing an amount, so the cents alone must
// decide it, whatever the whole part and the sub-cent digits are.
test("the cents of the amount pick the failure case", () => {
  expect(triggerOf(cash(25n, 13n))).toBe("refund");
  expect(triggerOf(cash(4_999n, 23n) + 9_999n)).toBe("retry");
  expect(triggerOf(cash(10n, 33n))).toBe("slow");
  expect(triggerOf(cash(25n, 0n))).toBeUndefined();
  expect(triggerOf(cash(13n, 0n))).toBeUndefined();
  expect(triggerOf(undefined)).toBeUndefined();
});

// What a session saves is all a restarted worker has, so it must read back
// as written, and anything unreadable must start the session afresh.
test("saved state reads back, and unreadable state starts afresh", () => {
  const saved = {
    v: 1 as const,
    amount: "25130000",
    inbound: {
      phase: "converting" as const,
      claims: [{ id: "0x01" as const, amount: "1", attempt: 0 }],
    },
  };
  expect(readSaved(writeSaved(saved))).toEqual(saved);
  expect(readSaved("0x6f5f31")).toEqual({ v: 1 });
  expect(readSaved(undefined)).toEqual({ v: 1 });
});
