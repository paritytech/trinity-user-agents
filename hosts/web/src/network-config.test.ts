import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  DEFAULT_NETWORK_CONFIG,
  NETWORKS,
  networkConfig,
} from "./network-config.js";

describe("NETWORKS", () => {
  test("paseo is Paseo Next v2 on the paseo TLD", () => {
    expect(NETWORKS.paseo).toEqual({
      id: "paseo",
      displayName: "Paseo Next v2",
      catalogNetwork: "paseo-next-v2",
      networkSuffix: "paseo",
      webGateway: "paseo.li",
      dotns: {
        assetHubRpc: "wss://paseo-asset-hub-next-rpc.polkadot.io",
        contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
        contenthashSlot: 0,
        contentGateway: "https://paseo-bulletin-next-ipfs.polkadot.io",
      },
    });
  });

  test("previewnet is PreviewNet on the testnet TLD", () => {
    expect(NETWORKS.previewnet).toEqual({
      id: "previewnet",
      displayName: "PreviewNet",
      catalogNetwork: "previewnet",
      networkSuffix: "testnet",
      webGateway: "testnet.li",
      dotns: {
        assetHubRpc: "wss://previewnet.substrate.dev/asset-hub",
        contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
        contenthashSlot: 0,
        contentGateway: "https://previewnet.substrate.dev",
      },
    });
  });

  // The resolver contract is the same CREATE3 deployment on both networks, so
  // a copy that drifts apart is a mistake, not a network difference.
  test("both networks read the same DotNS resolver contract", () => {
    expect(NETWORKS.previewnet.dotns.contentResolver).toBe(
      NETWORKS.paseo.dotns.contentResolver,
    );
  });

  // The core validates `network_suffix` against `DOTNS_TLDS`, so a suffix this
  // host offers but the core rejects would fail at boot.
  test("every network suffix is a dotNS TLD the core lists", () => {
    const platform = readFileSync(
      new URL("../../../rust/crates/truapi/src/platform.rs", import.meta.url),
      "utf8",
    );
    const list = /pub const DOTNS_TLDS: &\[&str\] = &\[([^\]]*)\];/.exec(
      platform,
    );
    const tlds = [...(list?.[1] ?? "").matchAll(/"([a-z]+)"/g)].map(
      (match) => match[1],
    );
    for (const network of Object.values(NETWORKS))
      expect(tlds).toContain(network.networkSuffix);
  });

  // A catalog name that is not in the bundled provider is only found when the
  // light client is asked to register it, which happens at boot.
  test("every catalog network is bundled in the provider catalog", () => {
    const catalog = readFileSync(
      new URL(
        "../../../rust/crates/truapi-provider/src/networks.rs",
        import.meta.url,
      ),
      "utf8",
    );
    const names = [...catalog.matchAll(/name: "([a-z0-9-]+)"/g)].map(
      (match) => match[1],
    );
    for (const network of Object.values(NETWORKS))
      expect(names).toContain(network.catalogNetwork);
  });
});

describe("networkConfig", () => {
  test("returns the default when no id is given", () => {
    expect(networkConfig()).toBe(DEFAULT_NETWORK_CONFIG);
    expect(networkConfig(null)).toBe(DEFAULT_NETWORK_CONFIG);
    expect(networkConfig("")).toBe(DEFAULT_NETWORK_CONFIG);
    expect(networkConfig("  ")).toBe(DEFAULT_NETWORK_CONFIG);
    expect(DEFAULT_NETWORK_CONFIG.id).toBe("paseo");
  });

  test("resolves a known id, trimming around it", () => {
    expect(networkConfig("previewnet")).toBe(NETWORKS.previewnet);
    expect(networkConfig(" previewnet ")).toBe(NETWORKS.previewnet);
  });

  // Serving the default on a typo would connect to the wrong network silently.
  test("throws for an unknown id instead of falling back", () => {
    expect(() => networkConfig("preview")).toThrow("Unknown network");
    expect(() => networkConfig("toString")).toThrow("Unknown network");
  });
});