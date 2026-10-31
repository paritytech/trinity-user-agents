import { describe, expect, test } from "bun:test";
import { demoAllowed } from "../../../../tools/demos/local-demo-route.js";

describe("the local demos", () => {
  test("are offered only by a dev server on this machine", () => {
    expect(demoAllowed(true, "localhost")).toBe(true);
    expect(demoAllowed(true, "127.0.0.1")).toBe(true);
    expect(demoAllowed(false, "localhost")).toBe(false);
    expect(demoAllowed(true, "192.168.1.20")).toBe(false);
    expect(demoAllowed(true, "host.example")).toBe(false);
  });
});
