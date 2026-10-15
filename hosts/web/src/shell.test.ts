import { describe, expect, test } from "bun:test";
import { NETWORKS } from "./network-config.js";
import {
  defaultAddresses,
  destinationOf,
  loadAddresses,
  resolveNavigation,
  saveAddress,
} from "./shell.js";

const PASEO = NETWORKS.paseo;

function memory(): Pick<Storage, "getItem" | "setItem"> & {
  values: Map<string, string>;
} {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, value),
  };
}

describe("dock addresses", () => {
  // Pocket is a screen the host draws, so it has no product address to keep.
  test("start with the published products and keep none for Pocket", () => {
    expect(defaultAddresses("paseo")).toEqual({
      chat: "chat-spa-verbosity.paseo",
      browse: "browse.paseo",
    });
  });

  test("start empty on a network with no known names", () => {
    expect(defaultAddresses("previewnet")).toEqual({ chat: "", browse: "" });
  });

  test("keep what a destination was last pointed at, per network", () => {
    const storage = memory();
    saveAddress(storage, "paseo", "browse", "http://localhost:5192/");
    expect(loadAddresses(storage, "paseo").browse).toBe(
      "http://localhost:5192/",
    );
    expect(loadAddresses(storage, "previewnet").browse).toBe("");
  });

  // The default network keeps the original key, so a tab from before the dock
  // existed shares nothing with it by accident and nothing is renamed later.
  test("use the shared key prefix of the host's other storage", () => {
    const storage = memory();
    saveAddress(storage, "paseo", "chat", "other.paseo");
    saveAddress(storage, "previewnet", "chat", "other.testnet");
    expect([...storage.values.keys()].sort()).toEqual([
      "truapi-web-signing-host:dock:v1",
      "truapi-web-signing-host:network:previewnet:dock:v1",
    ]);
  });

  test("do not keep an address that carries a secret", () => {
    const storage = memory();
    saveAddress(storage, "paseo", "browse", "http://localhost:1/?token=abc");
    expect(storage.values.size).toBe(0);
    expect(loadAddresses(storage, "paseo").browse).toBe("browse.paseo");
  });

  // A dock saved before Pocket became a host screen still carries a pocket entry.
  test("ignore a saved address for a destination the host draws itself", () => {
    const storage = memory();
    storage.setItem(
      "truapi-web-signing-host:dock:v1",
      '{"chat":"a.paseo","pocket":"http://localhost:5192/"}',
    );
    expect(loadAddresses(storage, "paseo")).toEqual({
      chat: "a.paseo",
      browse: "browse.paseo",
    });
  });

  test("fall back to the defaults when the saved value is unreadable", () => {
    const storage = memory();
    storage.setItem("truapi-web-signing-host:dock:v1", "{oops");
    expect(loadAddresses(storage, "paseo")).toEqual(defaultAddresses("paseo"));
    storage.setItem(
      "truapi-web-signing-host:dock:v1",
      '{"chat":7,"browse":"b.paseo"}',
    );
    expect(loadAddresses(storage, "paseo")).toEqual({
      ...defaultAddresses("paseo"),
      browse: "b.paseo",
    });
  });
});

describe("resolveNavigation", () => {
  // Browse hands a product to the host as https://<name>.<tld>. That is the
  // case the dock exists for, so it must stay in the host.
  test("opens a name of this network inside the host, whatever the scheme", () => {
    for (const url of [
      "https://doom.paseo",
      "https://doom.paseo/",
      "polkadot://doom.paseo",
      "polkadotapp://doom.paseo",
    ])
      expect(resolveNavigation(url, PASEO)).toEqual({
        kind: "product",
        address: "doom.paseo",
      });
  });

  test("keeps the path, query and hash of a name", () => {
    expect(
      resolveNavigation("polkadot://doom.paseo/room/7?x=1#top", PASEO),
    ).toEqual({
      kind: "product",
      address: "doom.paseo/room/7?x=1#top",
    });
    expect(resolveNavigation("https://doom.paseo/?x=1", PASEO)).toEqual({
      kind: "product",
      address: "doom.paseo/?x=1",
    });
  });

  test("leaves an ordinary web address to the browser", () => {
    expect(resolveNavigation("https://example.com/a", PASEO)).toEqual({
      kind: "external",
      url: "https://example.com/a",
    });
    expect(resolveNavigation("http://localhost:3000", PASEO)).toEqual({
      kind: "external",
      url: "http://localhost:3000",
    });
  });

  // The old host opened every non-polkadot address with window.open, which
  // runs a javascript: address. Only http and https may leave the host.
  test("refuses an address that is neither a product nor a web page", () => {
    for (const url of [
      "javascript:alert(1)",
      "file:///etc/passwd",
      "polkadotapp://pair?handshake=00",
      "polkadot://localhost",
    ])
      expect(resolveNavigation(url, PASEO).kind).toBe("refused");
  });

  test("refuses a product of another network and says so", () => {
    const refused = resolveNavigation("polkadot://doom.dot", PASEO);
    expect(refused.kind).toBe("refused");
    expect(refused.kind === "refused" && refused.reason).toContain(
      "another network",
    );
  });

  test("sends a web page of another network's name to the browser as before", () => {
    expect(resolveNavigation("https://doom.dot", PASEO).kind).toBe("external");
  });
});

describe("destinationOf", () => {
  const addresses = {
    chat: "chat-spa-verbosity.paseo",
    browse: "browse.paseo",
  };

  test("finds the destination that already holds a product", () => {
    expect(destinationOf("browse.paseo", addresses, PASEO)).toBe("browse");
    expect(destinationOf("browse", addresses, PASEO)).toBe("browse");
  });

  test("does not match a name that carries its own path", () => {
    expect(destinationOf("browse.paseo/app/x", addresses, PASEO)).toBeNull();
  });

  test("does not match a URL or an unknown name", () => {
    expect(
      destinationOf("http://localhost:5192/", addresses, PASEO),
    ).toBeNull();
    expect(destinationOf("doom.paseo", addresses, PASEO)).toBeNull();
    expect(destinationOf("???", addresses, PASEO)).toBeNull();
  });
});
