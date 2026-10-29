import { describe, expect, test } from "bun:test";
import { keyboardArea } from "../../../src/app/keyboard-viewport.js";

describe("keyboardArea", () => {
  // iOS Safari keeps the layout viewport at 844 and only shrinks what shows.
  test("is the visible area when a keyboard covers the bottom of a layout that did not shrink", () => {
    expect(keyboardArea({ height: 520, offsetTop: 0, scale: 1 }, 844)).toEqual({
      height: 520,
      top: 0,
    });
  });

  test("follows the area the browser panned to", () => {
    expect(keyboardArea({ height: 520, offsetTop: 60, scale: 1 }, 844)).toEqual(
      { height: 520, top: 60 },
    );
  });

  // Chrome on Android resizes the layout, so adding the inset again would
  // leave a gap the size of the keyboard.
  test("adds nothing when the layout shrank with the keyboard", () => {
    expect(
      keyboardArea({ height: 520, offsetTop: 0, scale: 1 }, 520),
    ).toBeNull();
  });

  test("adds nothing when nothing covers the page", () => {
    expect(
      keyboardArea({ height: 844, offsetTop: 0, scale: 1 }, 844),
    ).toBeNull();
  });

  // A pinch zoom also shrinks the visible area, and must stay reachable.
  test("adds nothing while the page is zoomed", () => {
    expect(
      keyboardArea({ height: 422, offsetTop: 0, scale: 2 }, 844),
    ).toBeNull();
  });
});
