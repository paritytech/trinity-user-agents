import { describe, expect, test } from "bun:test";
import {
  MAX_SIDE,
  MIN_SIDE,
  clampSide,
  draggedSize,
  loadCustomSize,
} from "./viewport.js";

describe("custom viewport size", () => {
  test("holds a side between the bounds and rounds it", () => {
    expect(clampSide(10, 480)).toBe(MIN_SIDE);
    expect(clampSide(99999, 480)).toBe(MAX_SIDE);
    expect(clampSide(300.6, 480)).toBe(301);
    expect(clampSide(Number.NaN, 480)).toBe(480);
  });

  // The pointer moves on screen, but the frame is drawn at `factor` of its
  // size, so the same screen delta is more CSS pixels when it is shrunk.
  test("turns a screen drag into CSS pixels at the drawn scale", () => {
    const start = { width: 400, height: 600 };
    expect(draggedSize(start, { x: 50, y: -30 }, 1)).toEqual({
      width: 450,
      height: 570,
    });
    expect(draggedSize(start, { x: 50, y: -30 }, 0.5)).toEqual({
      width: 500,
      height: 540,
    });
  });

  test("never drags past the bounds or the room the stage has", () => {
    const start = { width: 400, height: 600 };
    expect(draggedSize(start, { x: -9999, y: -9999 }, 1)).toEqual({
      width: MIN_SIDE,
      height: MIN_SIDE,
    });
    expect(
      draggedSize(start, { x: 9999, y: 9999 }, 1, { width: 700, height: 900 }),
    ).toEqual({ width: 700, height: 900 });
  });

  test("reads a saved size, and falls back when it is damaged or out of range", () => {
    const store = (value: string | null) => ({ getItem: () => value });
    expect(loadCustomSize(store('{"width":600,"height":800}'))).toEqual({
      width: 600,
      height: 800,
    });
    expect(loadCustomSize(store('{"width":5,"height":99999}'))).toEqual({
      width: MIN_SIDE,
      height: MAX_SIDE,
    });
    for (const bad of [null, "{oops", "7", '{"width":"x"}'])
      expect(loadCustomSize(store(bad))).toEqual({ width: 480, height: 720 });
  });
});
