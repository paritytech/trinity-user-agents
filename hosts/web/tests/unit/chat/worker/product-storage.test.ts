import { describe, expect, test } from "bun:test";
import { KeptStorage } from "../../../support/product-storage.js";

describe("the worker's product storage", () => {
  // A save that failed must be written again when it is saved again, even
  // unchanged: otherwise a product waits on it until its content changes.
  test("writes an unchanged value again after its write failed, and then flushes", async () => {
    const kept = new KeptStorage();
    const storage = await kept.open(["record"]);
    kept.failing = "the disk is full";
    storage.setItem("record", "one");
    const failed = await storage.flush().then(
      () => "flushed",
      (error: unknown) => String(error),
    );
    kept.failing = null;
    storage.setItem("record", "one");
    await storage.flush();

    expect([failed, kept.kept.has("record")]).toEqual([
      expect.stringContaining("the disk is full"),
      true,
    ]);
  });
});
