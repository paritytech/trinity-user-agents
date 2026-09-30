import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./style.css", import.meta.url), "utf8");

describe("host stylesheet", () => {
  // The inspector renders `<div class="ins-summary empty">` for its own empty
  // state. A host rule on a bare `.empty` positions that text over the host's
  // page, so the host must not style that class name.
  test("does not style the bare `empty` class the debugger uses", () => {
    const selectors = [...css.matchAll(/([^{}]+)\{/g)].flatMap((match) =>
      match[1].split(","),
    );
    const bare = selectors.filter((selector) =>
      /\.empty(?![\w-])/.test(selector),
    );
    expect(bare).toEqual([]);
  });
});
