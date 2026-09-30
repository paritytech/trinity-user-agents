import { describe, expect, test } from "bun:test";
import {
  NO_RELAXATIONS,
  activeRelaxations,
  loadRelaxations,
  saveRelaxations,
} from "./relaxations.js";

function memory(initial?: string) {
  let value = initial ?? null;
  return {
    getItem: () => value,
    setItem: (_key: string, next: string) => {
      value = next;
    },
  };
}

describe("relaxations", () => {
  test("a fresh tab has none on", () => {
    expect(loadRelaxations(memory())).toEqual(NO_RELAXATIONS);
    expect(activeRelaxations(NO_RELAXATIONS)).toEqual([]);
  });

  test("keeps what was saved", () => {
    const store = memory();
    saveRelaxations(store, {
      archiveWithoutContainer: true,
      approveNetworkWithoutAsking: false,
    });
    expect(loadRelaxations(store)).toEqual({
      archiveWithoutContainer: true,
      approveNetworkWithoutAsking: false,
    });
  });

  test("only a saved true turns one on, and damaged data turns none on", () => {
    expect(loadRelaxations(memory("not json"))).toEqual(NO_RELAXATIONS);
    expect(
      loadRelaxations(memory('{"archiveWithoutContainer":"yes"}')),
    ).toEqual(NO_RELAXATIONS);
    expect(loadRelaxations(memory("[1]"))).toEqual(NO_RELAXATIONS);
  });

  test("says what each one gives up", () => {
    const lines = activeRelaxations({
      archiveWithoutContainer: true,
      approveNetworkWithoutAsking: true,
    });
    expect(lines.length).toBe(2);
    expect(lines[0]).toContain("not gated");
    expect(lines[1]).toContain("without asking");
  });
});
