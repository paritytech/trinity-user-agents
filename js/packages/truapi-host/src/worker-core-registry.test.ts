import { describe, expect, it } from "bun:test";

import {
  dispatchFrame,
  disposeAwaitingFrames,
} from "./worker-core-registry.ts";

// A fake core modelling the wasm-bindgen borrow: free() throws while a
// receiveFrame promise is still unsettled, exactly as the real core does.
class FakeCore {
  disposed = false;
  freed = false;
  private inFlight = 0;
  private settlers: Array<() => void> = [];

  receiveFrame(_bytes: Uint8Array): Promise<void> {
    this.inFlight += 1;
    return new Promise<void>((resolve) => {
      this.settlers.push(() => {
        this.inFlight -= 1;
        resolve();
      });
    });
  }

  // dispose() aborts in-flight dispatch, so the awaited receiveFrame settles.
  dispose(): void {
    this.disposed = true;
    this.settlers.splice(0).forEach((settle) => settle());
  }

  free(): void {
    if (this.inFlight > 0) {
      throw new Error(
        "attempted to take ownership of Rust value while it was borrowed",
      );
    }
    this.freed = true;
  }
}

describe("worker core lifecycle", () => {
  it("disposes mid-frame without throwing and frees the core", async () => {
    const inFlightFrames = new Map<number, Set<Promise<void>>>();
    const core = new FakeCore();

    // A frame is dispatched and still in flight, so the borrow is held.
    const frame = dispatchFrame(core, 1, new Uint8Array([1]), inFlightFrames);
    expect(inFlightFrames.get(1)?.size).toBe(1);

    // Disposing mid-flight aborts the frame, then frees once the borrow releases.
    await disposeAwaitingFrames(core, 1, inFlightFrames);

    expect(core.disposed).toBe(true);
    expect(core.freed).toBe(true);
    expect(inFlightFrames.has(1)).toBe(false);
    await frame;
  });

  it("free() throws while a frame is borrowed, so the ordering matters", () => {
    // Guards that the test above is meaningful: freeing before the frame
    // settles is exactly the borrow error the ordering avoids.
    const core = new FakeCore();
    void core.receiveFrame(new Uint8Array());
    expect(() => core.free()).toThrow("borrowed");
  });
});
