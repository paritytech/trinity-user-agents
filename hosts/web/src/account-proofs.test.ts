import { describe, expect, test } from "bun:test";
import { refused } from "./account-proofs.js";

describe("account proof refusals", () => {
  test("name the core's domain error, so a missing grant is told apart", () => {
    expect(
      refused({
        tag: "Domain",
        value: { tag: "V1", value: { tag: "NotAllowlisted" } },
      }).tag,
    ).toBe("NotAllowlisted");
    expect(
      refused({
        tag: "Domain",
        value: {
          tag: "V1",
          value: { tag: "Unknown", value: { reason: "no ring" } },
        },
      }).message,
    ).toBe("Unknown: no ring");
    expect(
      refused({ tag: "HostFailure", value: { reason: "down" } }).message,
    ).toBe("HostFailure: down");
    expect(refused({ tag: "Denied" }).tag).toBe("Denied");
  });
});
