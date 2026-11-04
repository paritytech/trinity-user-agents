import { describe, expect, test } from "bun:test";
import { MemoryStorage } from "../../support/test-support.js";
import {
  TAB_NETWORK_KEY,
  chainBackendsKey,
  chosenNetwork,
  rememberNetwork,
  saveChainBackends,
  savedChainBackends,
  tabProductKey,
  tabWalletKey,
  type NetworkChoice,
} from "../../../src/platform/network-choice.js";

const OFFERED: NetworkChoice[] = [
  { id: "alpha", label: "Alpha" },
  { id: "beta", label: "Beta" },
];

describe("tab network choice", () => {
  // A tab from before the choice existed has nothing saved and must keep
  // running on the fallback network, not be moved to the first offered one.
  test("falls back when the tab has no saved choice", () => {
    expect(chosenNetwork(new MemoryStorage(), OFFERED, "alpha")).toBe("alpha");
  });

  test("returns the tab's saved choice when it is still offered", () => {
    const storage = new MemoryStorage();
    storage.setItem(TAB_NETWORK_KEY, "beta");
    expect(chosenNetwork(storage, OFFERED, "alpha")).toBe("beta");
  });

  // A saved id this build does not serve must not select an unknown network.
  test("falls back when the saved choice is not offered", () => {
    const storage = new MemoryStorage();
    storage.setItem(TAB_NETWORK_KEY, "gamma");
    expect(chosenNetwork(storage, OFFERED, "alpha")).toBe("alpha");
  });

  test("remembers the choice under the tab key", () => {
    const storage = new MemoryStorage();
    rememberNetwork(storage, "beta");
    expect(storage.getItem(TAB_NETWORK_KEY)).toBe("beta");
    expect(chosenNetwork(storage, OFFERED, "alpha")).toBe("beta");
  });

  // Per tab: a choice written in one tab is not visible to another, which is
  // what keeps two tabs on different networks.
  test("keeps the choice per storage, so tabs do not share it", () => {
    const first = new MemoryStorage();
    const second = new MemoryStorage();
    rememberNetwork(first, "beta");
    expect(chosenNetwork(second, OFFERED, "alpha")).toBe("alpha");
  });
});

describe("tab wallet and product keys", () => {
  // A tab from before networks were separated must restore its wallet and
  // product unchanged, so the default network keeps the original keys.
  test("keeps the legacy keys for the default network", () => {
    expect(tabWalletKey("paseo")).toBe("truapi-web-signing-host:tab-wallet");
    expect(tabProductKey("paseo")).toBe("truapi-web-signing-host:tab-product");
    expect(tabWalletKey()).toBe("truapi-web-signing-host:tab-wallet");
    expect(tabProductKey()).toBe("truapi-web-signing-host:tab-product");
  });

  // Another network gets its own keys, so a product left open on one network
  // is never reopened as the session on another.
  test("namespaces the keys for another network", () => {
    expect(tabWalletKey("previewnet")).toBe(
      "truapi-web-signing-host:network:previewnet:tab-wallet",
    );
    expect(tabProductKey("previewnet")).toBe(
      "truapi-web-signing-host:network:previewnet:tab-product",
    );
  });

  test("gives two networks different keys", () => {
    expect(tabProductKey("paseo")).not.toBe(tabProductKey("previewnet"));
    expect(tabWalletKey("paseo")).not.toBe(tabWalletKey("previewnet"));
  });
});
describe("tab chain backends", () => {
  test("saves and reads a tab's choice per network", () => {
    const storage = new MemoryStorage();
    saveChainBackends(storage, "paseo", { people: "light-client" });
    expect(savedChainBackends(storage, "paseo")).toEqual({
      people: "light-client",
    });
    expect(savedChainBackends(storage, "previewnet")).toEqual({});
  });

  // A choice saved by another build, or edited by hand, must not reach the
  // provider as an unknown backend or chain.
  test("drops unknown chains and backends, and unreadable values", () => {
    const storage = new MemoryStorage();
    storage.setItem(
      chainBackendsKey("paseo"),
      JSON.stringify({ people: "smoldot", relay: "rpc", coretime: "rpc" }),
    );
    expect(savedChainBackends(storage, "paseo")).toEqual({ relay: "rpc" });
    storage.setItem(chainBackendsKey("paseo"), "{not json");
    expect(savedChainBackends(storage, "paseo")).toEqual({});
  });

  test("removes the saved choice when no chain differs from its default", () => {
    const storage = new MemoryStorage();
    saveChainBackends(storage, "paseo", { relay: "rpc" });
    saveChainBackends(storage, "paseo", {});
    expect(storage.getItem(chainBackendsKey("paseo"))).toBeNull();
  });
});
