import { describe, expect, test } from "bun:test";
import {
  WorkerConsoleLog,
  lineText,
  passes,
} from "../../../../tools/workers/console-log.js";
import type { WorkerConsoleEntry } from "../../../../src/products/worker-console.js";

const entry = (
  text: string,
  level: WorkerConsoleEntry["level"] = "log",
): WorkerConsoleEntry => ({ level, source: "console", text, at: 0 });

describe("the workers' console log", () => {
  test("keeps each worker's lines apart, labelled with their run", () => {
    const log = new WorkerConsoleLog();
    log.console({ name: "a.paseo", run: 1 }, entry("a first"));
    log.console({ name: "b.paseo", run: 2 }, entry("b first"));
    log.console({ name: "a.paseo", run: 3 }, entry("a after restart"));
    expect(log.lines("a.paseo").map(({ text, run }) => [text, run])).toEqual([
      ["a first", 1],
      ["a after restart", 3],
    ]);
    expect(log.lines("b.paseo").map((line) => line.text)).toEqual(["b first"]);
  });

  test("keeps at most its limit per worker, the oldest dropped first", () => {
    const log = new WorkerConsoleLog(3);
    for (let index = 1; index <= 5; index++)
      log.console({ name: "a.paseo", run: 1 }, entry(`line ${index}`));
    expect(log.lines("a.paseo").map((line) => line.text)).toEqual([
      "line 3",
      "line 4",
      "line 5",
    ]);
    expect(log.dropped("a.paseo")).toBe(2);
  });

  // A new wallet or network must not see what the last session's workers wrote.
  test("forgets everything when the session changes", () => {
    const log = new WorkerConsoleLog();
    log.console({ name: "a.paseo", run: 1 }, entry("old session"));
    log.sessionChanged();
    expect(log.lines("a.paseo")).toEqual([]);
    expect(log.dropped("a.paseo")).toBe(0);
  });

  test("forgets a worker that left the session, and one cleared by hand", () => {
    const log = new WorkerConsoleLog();
    log.console({ name: "a.paseo", run: 1 }, entry("a"));
    log.console({ name: "b.paseo", run: 2 }, entry("b"));
    log.keepOnly(new Set(["b.paseo"]));
    expect(log.lines("a.paseo")).toEqual([]);
    log.clear("b.paseo");
    expect(log.lines("b.paseo")).toEqual([]);
  });

  test("tells listeners about each change", () => {
    const log = new WorkerConsoleLog();
    let changes = 0;
    log.onChange(() => (changes += 1));
    log.console({ name: "a.paseo", run: 1 }, entry("a"));
    log.keepOnly(new Set(["a.paseo"]));
    log.clear("a.paseo");
    log.sessionChanged();
    expect(changes).toBe(3);
  });

  test("filters by level and copies a line with its run", () => {
    const log = new WorkerConsoleLog();
    log.console({ name: "a.paseo", run: 4 }, entry("fine"));
    log.console({ name: "a.paseo", run: 4 }, entry("careful", "warn"));
    log.console(
      { name: "a.paseo", run: 4 },
      {
        ...entry("boom", "error"),
        source: "error",
      },
    );
    const lines = log.lines("a.paseo");
    expect(lines.filter((line) => passes(line, "warnings")).length).toBe(2);
    expect(lines.filter((line) => passes(line, "errors")).length).toBe(1);
    expect(lineText(lines[2])).toEndWith("run 4 error (uncaught) boom");
  });
});
