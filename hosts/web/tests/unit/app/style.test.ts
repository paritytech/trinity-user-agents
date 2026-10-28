import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const css = readFileSync(
  new URL("../../../src/style.css", import.meta.url),
  "utf8",
);
const page = readFileSync(
  new URL("../../../index.html", import.meta.url),
  "utf8",
);

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

  // The dev server adds this stylesheet from the page's script, so the page
  // paints once before it applies. An inline icon with no size of its own then
  // fills the page's width: the brand mark showed as a page of black dots.
  test("every inline icon in the page has a size before the stylesheet applies", () => {
    const icons = [...page.matchAll(/<svg\b[^>]*>/g)].map((match) => match[0]);
    expect(icons.length).toBeGreaterThan(0);
    expect(
      icons.filter(
        (icon) => !/\swidth="\d+"/.test(icon) || !/\sheight="\d+"/.test(icon),
      ),
    ).toEqual([]);
  });
});
