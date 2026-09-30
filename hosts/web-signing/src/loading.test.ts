import { describe, expect, test } from "bun:test";
import { StageLoading, documentsBeforeProduct } from "./loading.js";

function loading() {
  const shown: (string | null)[] = [];
  return { stage: new StageLoading((text) => shown.push(text)), shown };
}

describe("stage loading", () => {
  test("shows from begin until that operation finishes", () => {
    const { stage, shown } = loading();
    const id = stage.begin("Loading…");
    expect(stage.text).toBe("Loading…");
    stage.finish(id);
    expect(stage.text).toBeNull();
    expect(shown).toEqual(["Loading…", null]);
  });

  // A slow frame load or error from an earlier open must not hide the loading
  // of the open that replaced it.
  test("ignores the completion of an operation that was replaced", () => {
    const { stage } = loading();
    const first = stage.begin("Looking up name…");
    const second = stage.begin("Loading…");
    stage.finish(first);
    expect(stage.text).toBe("Loading…");
    stage.finish(second);
    expect(stage.text).toBeNull();
  });

  test("ignores the completion of an operation that was closed", () => {
    const { stage } = loading();
    const closed = stage.begin("Loading…");
    stage.clear();
    const next = stage.begin("Loading…");
    stage.finish(closed);
    expect(stage.text).not.toBeNull();
    stage.finish(next);
  });

  test("tells the page only when something changed", () => {
    const { stage, shown } = loading();
    stage.clear();
    const id = stage.begin("Loading…");
    stage.finish(id);
    stage.finish(id);
    expect(shown).toEqual(["Loading…", null]);
  });

  // The loader page is the first document of a name, and is not the product.
  test("waits for the product's own document when opened by name", () => {
    expect(documentsBeforeProduct("name")).toBe(2);
    expect(documentsBeforeProduct("url")).toBe(1);
  });
});
