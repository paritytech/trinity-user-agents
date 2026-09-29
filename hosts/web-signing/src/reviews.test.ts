import { describe, expect, test } from "bun:test";
import { describeReview, formatValue } from "./reviews.js";

describe("describeReview", () => {
  // The protocol requires the host to warn that unwatermarked bytes could be
  // a transaction.
  test("warns about an unwatermarked raw signature", () => {
    const review = describeReview({
      tag: "SignRaw",
      value: {
        tag: "Product",
        value: {
          callingProductId: "myapp.paseo",
          request: {} as never,
          watermarked: false,
        },
      },
    });
    expect(review.fields.slice(0, 2)).toEqual([
      { label: "Requested by", value: "myapp.paseo" },
      {
        label: "Warning",
        value:
          "These bytes are not watermarked, so they could be a valid transaction.",
        warning: true,
      },
    ]);
  });

  test("does not warn about a watermarked raw signature", () => {
    const review = describeReview({
      tag: "SignRaw",
      value: {
        tag: "Product",
        value: {
          callingProductId: "myapp.paseo",
          request: {} as never,
          watermarked: true,
        },
      },
    });
    expect(review.fields.map((field) => field.label)).toEqual([
      "Requested by",
      "Request",
    ]);
  });

  test("names the product that asks for another product's account", () => {
    const review = describeReview({
      tag: "AccountAccess",
      value: { requestingProductId: "one.paseo", targetProductId: "two.paseo" },
    });
    expect(review.fields[0]).toEqual({
      label: "Requested by",
      value: "one.paseo",
    });
  });
});

describe("formatValue", () => {
  // Reviews carry bytes and u64 values, which plain JSON.stringify drops or
  // throws on.
  test("renders bytes as hex and bigints as decimal", () => {
    expect(
      formatValue({ data: new Uint8Array([0xde, 0xad]), amount: 10n ** 20n }),
    ).toBe('{\n  "data": "0xdead",\n  "amount": "100000000000000000000"\n}');
  });
});
