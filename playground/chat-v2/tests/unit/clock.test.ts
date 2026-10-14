import { describe, expect, test } from "bun:test";
import { Clock, EPOCH_MS, emptyClockState, historyId, historyTimestamp } from "../../worker/clock";

describe("Clock", () => {
  test("gives four-digit product ids per room and one timestamp counter", () => {
    const clock = new Clock(emptyClockState());
    expect(clock.nextProductId("host")).toEqual({ id: "host-p-0001", counter: 1 });
    expect(clock.nextProductId("source")).toEqual({ id: "source-p-0001", counter: 1 });
    expect(clock.nextProductId("host").id).toBe("host-p-0002");
    expect(clock.nextUserId()).toBe("source-u-0001");
    expect(clock.nextTimestamp()).toBe(1_767_225_601_000n);
    expect(clock.nextTimestamp()).toBe(1_767_225_602_000n);
    expect(clock.now()).toBe(1_767_225_602_000n);
  });

  test("two clocks from the same state give the same values", () => {
    const run = () => {
      const clock = new Clock(emptyClockState());
      return [clock.nextProductId("source").id, clock.nextUserId(), clock.nextTimestamp(), clock.now()];
    };
    expect(run()).toEqual(run());
  });

  test("continues from a saved state", () => {
    const state = emptyClockState();
    new Clock(state).nextProductId("host");
    const saved = JSON.parse(JSON.stringify(state));
    expect(new Clock(saved).nextProductId("host").id).toBe("host-p-0002");
  });

  test("history is older than every live message", () => {
    expect(historyId(17)).toBe("source-h-00017");
    expect(historyTimestamp(100, 100)).toBe(EPOCH_MS);
    expect(historyTimestamp(1, 100)).toBe(EPOCH_MS - 99n * 60_000n);
    expect(historyTimestamp(100, 100) < new Clock(emptyClockState()).nextTimestamp()).toBe(true);
  });
});
