import { describe, expect, test } from "bun:test";
import type { TrUApiClient } from "@parity/truapi";
import { runExample, type LogEntry } from "../../src/lib/example-runner";

describe("runExample", () => {
  // An example that opens with an rxjs import must still have it rewritten
  // when its body uses optional chaining or nullish coalescing, or every host
  // fails the example before it runs.
  test("runs an example that imports rxjs and uses `?.` and `??`", async () => {
    const source = [
      'import { firstValueFrom, of } from "rxjs";',
      "",
      "const locale = await firstValueFrom(",
      "  of({ timeZone: undefined as string | undefined }),",
      ");",
      'console.log(locale.timeZone?.toUpperCase() ?? "no time zone");',
    ].join("\n");
    const logs: LogEntry[] = [];

    const { promise } = await runExample({
      source,
      client: {} as TrUApiClient,
      onLog: (entry) => logs.push(entry),
    });
    await promise;

    expect(logs).toEqual([{ level: "log", text: "no time zone" }]);
  });
});
