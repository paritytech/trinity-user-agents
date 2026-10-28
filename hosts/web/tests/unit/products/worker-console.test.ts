import { describe as group, expect, test } from "bun:test";
import {
  MAX_ARGS,
  MAX_DEPTH,
  MAX_ENTRIES,
  MAX_LINE,
  MAX_STRING,
  describe,
  formatArgs,
} from "../../../src/products/loader/describe.js";
import {
  CONSOLE_TEXT_LIMIT,
  WORKER_CONSOLE,
  readConsoleMessage,
} from "../../../src/products/worker-console.js";

group("describing console arguments", () => {
  test("shows primitives as a console does, and strings unquoted at the top", () => {
    expect(formatArgs(["ready", 3, true, null, undefined, 7n])).toBe(
      "ready 3 true null undefined 7n",
    );
    expect(describe({ name: "ada", n: 1 })).toBe('{name: "ada", n: 1}');
    expect(describe([1, "two", [3]])).toBe('[1, "two", [3]]');
  });

  // A getter is the logged object's own code: reading it could change state or throw.
  test("does not run a getter, and says it is one", () => {
    let reads = 0;
    const value = {
      plain: 1,
      get secret() {
        reads += 1;
        return "read";
      },
    };
    expect(describe(value)).toBe("{plain: 1, secret: [Getter]}");
    expect(reads).toBe(0);
  });

  test("marks a value that refers back to one it is inside", () => {
    const loop: Record<string, unknown> = { id: 1 };
    loop.self = loop;
    loop.list = [loop];
    expect(describe(loop)).toBe(
      "{id: 1, self: [Circular], list: [[Circular]]}",
    );
    // A value seen twice side by side is not circular.
    const shared = { x: 1 };
    expect(describe([shared, shared])).toBe("[{x: 1}, {x: 1}]");
  });

  test("stops at its depth, entry and string limits", () => {
    let deep: unknown = "bottom";
    for (let level = 0; level < MAX_DEPTH + 3; level++) deep = { next: deep };
    expect(describe(deep)).toContain("[Object]");
    expect(describe(deep)).not.toContain("bottom");

    const wide = Object.fromEntries(
      Array.from({ length: MAX_ENTRIES + 5 }, (_, i) => [`k${i}`, i]),
    );
    expect(describe(wide)).toContain("… 5 more");

    const long = "x".repeat(MAX_STRING + 50);
    expect(describe(long)).toBe(`${"x".repeat(MAX_STRING)}… (50 more)`);
  });

  // Binary data and base64 blobs are what makes a console line huge.
  test("gives the size of binary data, never its bytes", () => {
    const bytes = new Uint8Array(100_000).fill(65);
    expect(describe(bytes)).toBe("Uint8Array(100000 bytes)");
    expect(describe(bytes.buffer)).toBe("ArrayBuffer(100000 bytes)");
    expect(describe(new DataView(new ArrayBuffer(8)))).toBe(
      "DataView(8 bytes)",
    );
    expect(describe(new Blob(["abc"]))).toBe("Blob(3 bytes)");
    expect(describe(new Map([[1, 2]]))).toBe("Map(1)");
    expect(describe(new Set([1, 2, 3]))).toBe("Set(3)");
  });

  test("keeps a whole line within its budget, however much is logged", () => {
    const big = Array.from({ length: 50 }, () =>
      Object.fromEntries(
        Array.from({ length: 50 }, (_, i) => [`k${i}`, "y".repeat(400)]),
      ),
    );
    const started = performance.now();
    const line = formatArgs([big, big, big]);
    expect(line.length).toBeLessThanOrEqual(MAX_LINE + 40);
    expect(performance.now() - started).toBeLessThan(200);
    expect(
      formatArgs(Array.from({ length: MAX_ARGS + 3 }, (_, i) => i)),
    ).toEndWith("… 3 more arguments");
  });

  test("describes an error by its name, message and stack", () => {
    const error = new TypeError("bad input");
    const text = describe(error);
    expect(text.startsWith("TypeError: bad input")).toBe(true);
    expect(describe({ cause: error })).toContain("TypeError: bad input");
  });

  test("names a class instance, a function and a date", () => {
    class Point {
      x = 1;
    }
    expect(describe(new Point())).toBe("Point {x: 1}");
    expect(describe(function named() {})).toBe("[Function named]");
    expect(describe(new Date(0))).toBe("1970-01-01T00:00:00.000Z");
    expect(describe(Object.create(null))).toBe("{}");
  });

  test("never throws for a value that throws when inspected", () => {
    const hostile = new Proxy(
      {},
      {
        ownKeys() {
          throw new Error("no");
        },
      },
    );
    expect(describe(hostile)).toBe("[Unreadable]");
  });
});

group("reading a console message from a worker frame", () => {
  const AT = 1_000;

  test("reads a well-formed line and stamps it with the host's time", () => {
    expect(
      readConsoleMessage(
        { type: WORKER_CONSOLE, level: "warn", source: "console", text: "hi" },
        AT,
      ),
    ).toEqual({ level: "warn", source: "console", text: "hi", at: AT });
  });

  // The worker's own code can post this shape, past the capture's budgets.
  test("bounds text the frame did not, and strips credentials from URLs", () => {
    const line = readConsoleMessage(
      {
        type: WORKER_CONSOLE,
        level: "log",
        source: "console",
        text: `see https://me:pw@a.test/?token=t ${"z".repeat(CONSOLE_TEXT_LIMIT * 2)}`,
      },
      AT,
    );
    expect(line!.text.length).toBeLessThan(CONSOLE_TEXT_LIMIT + 100);
    expect(line!.text).not.toContain("me:pw");
    expect(line!.text).not.toContain("token=t");
  });

  test("ignores anything else", () => {
    for (const data of [
      null,
      "text",
      { type: "other", level: "log", source: "console", text: "x" },
      { type: WORKER_CONSOLE, level: "fatal", source: "console", text: "x" },
      { type: WORKER_CONSOLE, level: "log", source: "made-up", text: "x" },
      { type: WORKER_CONSOLE, level: "log", source: "console", text: 1 },
    ])
      expect(readConsoleMessage(data, AT)).toBeNull();
  });
});
