import { describe, expect, test } from "bun:test";
import { releaseCloses } from "../../../src/app/sheet-handle.js";

describe("a sheet handle's drag", () => {
  // The Developer sheet on a phone is about 690 px tall, so a slow drag must
  // pass 120 px; a short prompt closes at a quarter of its height instead.
  test("closes past 120 px, or a quarter of a short sheet, when let go slowly", () => {
    expect(releaseCloses(119, 0, 690)).toBe(false);
    expect(releaseCloses(120, 0, 690)).toBe(true);
    expect(releaseCloses(59, 0, 240)).toBe(false);
    expect(releaseCloses(60, 0, 240)).toBe(true);
  });

  // A flick is how a sheet is thrown away: fast is enough, but a twitch is not.
  test("closes on a fast flick once it has moved 24 px", () => {
    expect(releaseCloses(30, 0.49, 690)).toBe(false);
    expect(releaseCloses(30, 0.5, 690)).toBe(true);
    expect(releaseCloses(23, 2, 690)).toBe(false);
  });

  test("never closes when it has not moved down", () => {
    expect(releaseCloses(0, 3, 690)).toBe(false);
  });
});
