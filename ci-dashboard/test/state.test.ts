import { describe, expect, test } from "bun:test";

import type { CheckResult } from "../src/checks";
import { advance, message } from "../src/state";

const up: CheckResult = {
  name: "People",
  url: "https://rpc.test/people",
  ok: true,
  detail: "block 1",
};
const down: CheckResult = { ...up, ok: false, detail: "timed out" };

describe("advance", () => {
  test("waits for a second failure before alerting", () => {
    const first = advance({}, [down]);
    expect(first.transitions).toEqual([]);

    const second = advance(first.state, [down]);
    expect(second.transitions).toEqual([{ kind: "down", result: down }]);

    const third = advance(second.state, [down]);
    expect(third.transitions).toEqual([]);
  });

  test("announces recovery only after an alert", () => {
    const blip = advance(advance({}, [down]).state, [up]);
    expect(blip.transitions).toEqual([]);

    const outage = advance(advance({}, [down]).state, [down]);
    const recovered = advance(outage.state, [up]);
    expect(recovered.transitions).toEqual([{ kind: "recovered", result: up }]);
    expect(recovered.state[up.url]).toEqual({ failures: 0, alerted: false });
  });

  test("drops entries that are no longer checked", () => {
    const { state } = advance(
      { "https://gone.test": { failures: 5, alerted: true } },
      [up],
    );
    expect(Object.keys(state)).toEqual([up.url]);
  });
});

describe("message", () => {
  test("is empty when nothing changed", () => {
    expect(message([], "https://dashboard.test")).toBeUndefined();
  });

  test("escapes HTML and links the dashboard", () => {
    const alert = message(
      [{ kind: "down", result: { ...down, detail: "<oops>" } }],
      "https://dashboard.test",
    );
    expect(alert?.text).toBe(
      "❌ People is down: <oops>\nDashboard: https://dashboard.test",
    );
    expect(alert?.html).toContain("is down: &lt;oops&gt;");
    expect(alert?.html).toContain(
      '<a href="https://dashboard.test">Dashboard</a>',
    );
  });
});
