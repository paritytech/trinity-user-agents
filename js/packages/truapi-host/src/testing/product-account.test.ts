import { asProductId } from "@parity/truapi";
import { describe, expect, it } from "bun:test";

import { mockTestStack } from "../web/create-mock-host.js";
import { productAccountAddress } from "./product-account.js";
import { wasmIsBuilt } from "./require-wasm.js";

const suite = wasmIsBuilt("testing/truapi_server.js")
  ? describe
  : describe.skip;

suite("the address a product account will be given", () => {
  // Captured from the running products: `product-sdk`'s contracts-demo and
  // tx-demo reported these after connecting to a host whose session was
  // activated from the `bob` dev account. Pinned as literals because the point
  // of deriving here is to get the same answer a host will, and a derivation
  // checked only against itself would agree with a host that disagreed. They
  // are the addresses of the `.dot` ids as written, which a stack on `dot`
  // serves.
  const CONTRACTS = "5DkXL5QxHcJV4mzcEfScZuS7HJYFzGVJHB1x3m8pLcwpBgja";
  const TX = "5GWoDJDcE2k6LV1aA2wntxwZf9DbES4Tq8423jDA333KqoG3";
  const onDot = {
    ...mockTestStack(),
    tld: "dot",
    reservedValues: { allowed: true },
  };

  it("matches what a host gives the product", async () => {
    expect(
      await productAccountAddress({
        account: "bob",
        productId: asProductId("contracts-demo.dot"),
        testStack: onDot,
      }),
    ).toBe(CONTRACTS);
    expect(
      await productAccountAddress({
        account: "bob",
        productId: asProductId("tx-demo.dot"),
        testStack: onDot,
      }),
    ).toBe(TX);
  });

  it("reads a dot name as the host's stack does", async () => {
    // The mock test stack is on `paseo`, where the name `tx-demo.dot` is the
    // product `tx-demo.paseo`. A suite that funded the account of the id as
    // written would fund one the product never signs with.
    const written = await productAccountAddress({
      account: "bob",
      productId: asProductId("tx-demo.dot"),
    });
    const served = await productAccountAddress({
      account: "bob",
      productId: asProductId("tx-demo.paseo"),
    });
    expect([written === served, written === TX]).toEqual([true, false]);
  });

  it("refuses a product the host's stack does not serve", async () => {
    await expect(
      productAccountAddress({
        account: "bob",
        productId: asProductId("tx-demo.testnet"),
      }),
    ).rejects.toThrow(
      "the paseo stack serves no product under the id tx-demo.testnet",
    );
  });

  it("gives each product its own account", async () => {
    // The product id is an input to the derivation, so one account's products
    // do not share a balance -- which is why funding is per (product, account).
    const contracts = await productAccountAddress({
      account: "bob",
      productId: asProductId("contracts-demo.dot"),
    });
    const tx = await productAccountAddress({
      account: "bob",
      productId: asProductId("tx-demo.dot"),
    });
    expect(contracts).not.toBe(tx);
  });

  it("gives each session root its own account", async () => {
    // The root is an input to the derivation, so the same product under a
    // different account is a different address -- which is why funding is per
    // (product, account) rather than per product.
    const asCharlie = await productAccountAddress({
      account: "charlie",
      productId: asProductId("contracts-demo.dot"),
      testStack: onDot,
    });
    expect(asCharlie).not.toBe(CONTRACTS);
  });

  it("gives each index its own account", async () => {
    const second = await productAccountAddress({
      account: "bob",
      productId: asProductId("contracts-demo.dot"),
      index: 1,
      testStack: onDot,
    });
    expect(second).not.toBe(CONTRACTS);
  });
});

describe("the index a derivation will accept", () => {
  // `u32` wraps, so an index the caller never meant encodes to a different
  // account. A suite meets that as an address it funded which turns out not to
  // be the one its product signs with -- the shape of failure this guard is
  // here to stop being silent.
  it("refuses an index u32 would reshape", async () => {
    const { checkDerivationIndex } = await import("./dev-accounts.js");
    expect(() => checkDerivationIndex(-1)).toThrow(/not a u32/);
    expect(() => checkDerivationIndex(1.5)).toThrow(/not a u32/);
    expect(() => checkDerivationIndex(2 ** 32)).toThrow(/not a u32/);
    expect(() => checkDerivationIndex(Number.NaN)).toThrow(/not a u32/);
  });

  it("accepts the whole u32 range", async () => {
    const { checkDerivationIndex } = await import("./dev-accounts.js");
    expect(checkDerivationIndex(0)).toBe(0);
    expect(checkDerivationIndex(0xff_ff_ff_ff)).toBe(0xff_ff_ff_ff);
  });
});
