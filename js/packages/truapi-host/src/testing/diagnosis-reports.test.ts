import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/**
 * Compare the two committed diagnosis reports under
 * `explorer/diagnosis-reports/spa/`, method by method.
 *
 * Both are written by the same generated battery -- the CLI's by
 * `scripts/battery.sh`, the mock's by `scripts/fidelity-report.ts` -- so the
 * rows line up and a difference means the two hosts answered differently on
 * the runs that produced them.
 *
 * What this reads is two files, and nothing regenerates them automatically:
 * neither script runs in CI, so a change to `createMockHost` does not reach
 * these assertions until someone reruns the report. It therefore guards the
 * committed comparison, not the live mock, and it is not a check on whether a
 * product sees the same protocol behaviour here as against a shipping host.
 * The surface guards (`mock-host-surface`, `test-host-surface`) are what run
 * against live code.
 */
function readReport(name: string): Map<string, "pass" | "fail"> {
  const path = fileURLToPath(
    new URL(
      `../../../../../explorer/diagnosis-reports/spa/${name}`,
      import.meta.url,
    ),
  );
  const rows = new Map<string, "pass" | "fail">();
  for (const line of readFileSync(path, "utf8").split("\n")) {
    const match = /^\|\s*`([^`]+)`\s*\|\s*(\S+)\s*\|/.exec(line);
    if (match) rows.set(match[1], match[2].includes("✅") ? "pass" : "fail");
  }
  return rows;
}

describe("committed diagnosis reports", () => {
  const mock = readReport("mock-host.md");
  const real = readReport("signing-host-cli.md");
  const shared = [...mock.keys()].filter((id) => real.has(id));

  it("parsed both reports", () => {
    // Without this a regex that matched nothing would make every assertion
    // below vacuously true -- the failure mode these guards exist to avoid.
    expect(mock.size).toBeGreaterThan(50);
    expect(real.size).toBeGreaterThan(50);
    expect(shared.length).toBeGreaterThan(50);
  });

  it("records no method the mock passed where a real host failed", () => {
    // The load-bearing property of the comparison. A mock that succeeds where
    // the shipping host errors teaches a product the wrong thing, and the test
    // that relies on it passes for a reason that will not survive contact with
    // production. Divergence in the other direction is a gap, which is
    // disappointing; this direction is a lie, which is worse.
    const falseGreens = shared.filter(
      (id) => mock.get(id) === "pass" && real.get(id) === "fail",
    );
    expect(falseGreens).toEqual([]);
  });

  it("records agreement on most of the surface", () => {
    const agree = shared.filter((id) => mock.get(id) === real.get(id));
    // A floor, not a target. Chain-routed methods fail in the mock report
    // because the generator closes the chain to make the battery terminate,
    // so exact agreement is not the expectation -- a collapse is the signal.
    expect(agree.length).toBeGreaterThanOrEqual(30);
  });
});
