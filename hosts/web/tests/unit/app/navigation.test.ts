import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  ProductNavigation,
  type NavigationDeps,
} from "../../../src/app/navigation.js";
import { defaultAddresses } from "../../../src/app/dock-defaults.js";
import { DEFAULT_NETWORK_CONFIG as NETWORK } from "../../../src/platform/network-config.js";
import { tabProductKey } from "../../../src/platform/network-choice.js";
import { MemoryStorage } from "../../support/test-support.js";

/**
 * What the page hands navigation decides what it opens: the dock's addresses
 * it is given, and whether the tab reopens a product the user did not choose
 * this time.
 */

const saved = (globalThis as { sessionStorage?: unknown }).sessionStorage;
beforeAll(() => {
  Object.assign(globalThis, { sessionStorage: new MemoryStorage() });
});
afterAll(() => {
  Object.assign(globalThis, { sessionStorage: saved });
});

/** Navigation over only what these checks read of it. */
const navigation = (autoOpens: boolean) =>
  new ProductNavigation({
    network: NETWORK,
    addresses: { ...defaultAddresses(NETWORK.id), browse: "example.paseo" },
    autoOpens,
  } as NavigationDeps);

describe("product navigation", () => {
  test("uses the dock addresses it is given, and reopens the tab's product only when the tab opens products by itself", () => {
    sessionStorage.setItem(
      tabProductKey(NETWORK.id),
      JSON.stringify({
        address: "example.paseo",
        productId: "example.paseo",
        destination: "browse",
      }),
    );
    expect({
      browse: navigation(true).dockAddresses.browse,
      reopens: navigation(true).readTabProduct()?.address,
      without: navigation(false).readTabProduct(),
    }).toEqual({
      browse: "example.paseo",
      reopens: "example.paseo",
      without: null,
    });
  });
});
