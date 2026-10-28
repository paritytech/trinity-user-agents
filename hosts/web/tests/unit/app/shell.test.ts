import { describe, expect, test } from "bun:test";
import { NETWORKS } from "../../../src/platform/network-config.js";
import {
  defaultAddresses,
  destinationOf,
  loadAddresses,
  resolveNavigation,
} from "../../../src/app/shell.js";

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
  // Chat and Pocket are screens the host draws, so they have no product address to keep.
  test("start with the published products and keep none for Chat or Pocket", () => {
    expect(defaultAddresses("paseo")).toEqual({ browse: "browse.paseo" });
  });

  test("start empty on a network with no known names", () => {
    expect(defaultAddresses("previewnet")).toEqual({ browse: "" });
  });

  // The default network keeps the shorter key, as the host's other storage
  // does; another network has its own, so an address stays on its network.
  test("read a saved address per network, under the host's key prefix", () => {
    const storage = memory();
    storage.setItem(
      "truapi-web-signing-host:dock:v2",
      '{"browse":"http://localhost:5192/"}',
    );
    expect(loadAddresses(storage, "paseo").browse).toBe(
      "http://localhost:5192/",
    );
    expect(loadAddresses(storage, "previewnet").browse).toBe("");
    storage.setItem(
      "truapi-web-signing-host:network:previewnet:dock:v2",
      '{"browse":"other.testnet"}',
    );
    expect(loadAddresses(storage, "previewnet").browse).toBe("other.testnet");
  });

  // Chat and Pocket are screens the host draws, so only Browse can point elsewhere.
  test("ignore a saved address for a destination the host draws itself", () => {
    const storage = memory();
    storage.setItem(
      "truapi-web-signing-host:dock:v2",
      '{"chat":"a.paseo","pocket":"http://localhost:5192/"}',
    );
    expect(loadAddresses(storage, "paseo")).toEqual({ browse: "browse.paseo" });
  });

  // Earlier builds wrote the last product opened in Browse under v1, so
  // reading it would show that product as Browse's own.
  test("ignore the address earlier builds saved for Browse", () => {
    const storage = memory();
    storage.setItem(
      "truapi-web-signing-host:dock:v1",
      '{"browse":"doom.paseo"}',
    );
    expect(loadAddresses(storage, "paseo")).toEqual({ browse: "browse.paseo" });
  });

  test("fall back to the defaults when the saved value is unreadable", () => {
    const storage = memory();
    storage.setItem("truapi-web-signing-host:dock:v2", "{oops");
    expect(loadAddresses(storage, "paseo")).toEqual(defaultAddresses("paseo"));
    storage.setItem("truapi-web-signing-host:dock:v2", '{"browse":7}');
    expect(loadAddresses(storage, "paseo")).toEqual(defaultAddresses("paseo"));
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
  const addresses = { browse: "browse.paseo" };

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
