import { describe, expect, test } from "bun:test";
import { copyText } from "./copy.js";

describe("copyText", () => {
  test("writes the text to the clipboard and reports it", async () => {
    const written: string[] = [];
    const ok = await copyText("0xfull", {
      clipboard: {
        writeText: (text) => (written.push(text), Promise.resolve()),
      },
    });
    expect(ok).toBe(true);
    expect(written).toEqual(["0xfull"]);
  });

  // A refused write must not read as success; the fallback gets one try.
  test("falls back when the clipboard refuses, and fails when nothing works", async () => {
    const refuse = { writeText: () => Promise.reject(new Error("denied")) };
    expect(
      await copyText("x", { clipboard: refuse, execCopy: () => true }),
    ).toBe(true);
    expect(
      await copyText("x", { clipboard: refuse, execCopy: () => false }),
    ).toBe(false);
    expect(await copyText("x", { clipboard: refuse })).toBe(false);
    expect(await copyText("x", {})).toBe(false);
    expect(
      await copyText("x", {
        execCopy: () => {
          throw new Error("blocked");
        },
      }),
    ).toBe(false);
  });

  test("uses the fallback in a page without a clipboard", async () => {
    let copied = "";
    expect(
      await copyText("p", {
        execCopy: (text) => ((copied = text), true),
      }),
    ).toBe(true);
    expect(copied).toBe("p");
  });
});
