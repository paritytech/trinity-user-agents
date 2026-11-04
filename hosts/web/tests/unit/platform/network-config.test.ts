import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  DEFAULT_NETWORK_CONFIG,
  NETWORKS,
  chainBackends,
  networkConfig,
} from "../../../src/platform/network-config.js";
import { hostConnectionTypes } from "../../../src/platform/network.js";

describe("NETWORKS", () => {
  test("paseo is Paseo Next v2 on the paseo TLD", () => {
    expect(NETWORKS.paseo).toEqual({
      id: "paseo",
      displayName: "Paseo Next v2",
      catalogNetwork: "paseo-next-v2",
      networkSuffix: "paseo",
      webGateway: "paseo.li",
      chains: {
        relay: {
          rpc: "wss://paseo-rpc.n.dwellir.com",
          backend: "light-client",
        },
        assetHub: {
          rpc: "wss://paseo-asset-hub-next-rpc.polkadot.io",
          backend: "rpc",
        },
        people: {
          rpc: "wss://paseo-people-next-system-rpc.polkadot.io",
          backend: "rpc",
        },
        bulletin: {
          rpc: "wss://paseo-bulletin-next-rpc.polkadot.io",
          backend: "rpc",
        },
      },
      dotns: {
        contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
        contenthashSlot: 0,
        textSlot: 1,
        contentGateway: "https://paseo-bulletin-next-ipfs.polkadot.io",
      },
      identityBackend: "https://identity.dotspark.app",
    });
  });

  test("previewnet is PreviewNet on the testnet TLD", () => {
    expect(NETWORKS.previewnet).toEqual({
      id: "previewnet",
      displayName: "PreviewNet",
      catalogNetwork: "previewnet",
      networkSuffix: "testnet",
      webGateway: "testnet.li",
      chains: {
        relay: {
          rpc: "wss://previewnet.substrate.dev/relay/alice",
          backend: "light-client",
        },
        assetHub: {
          rpc: "wss://previewnet.substrate.dev/asset-hub",
          backend: "rpc",
        },
        people: {
          rpc: "wss://previewnet.substrate.dev/people",
          backend: "rpc",
        },
        bulletin: {
          rpc: "wss://previewnet.substrate.dev/bulletin",
          backend: "rpc",
        },
      },
      dotns: {
        contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
        contenthashSlot: 0,
        textSlot: 1,
        contentGateway: "https://previewnet.substrate.dev",
      },
      identityBackend: "https://identity-previewnet.dotspark.app",
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
    const dotnsName = readFileSync(
      new URL(
        "../../../../../rust/crates/truapi/src/dotns_name.rs",
        import.meta.url,
      ),
      "utf8",
    );
    const list = /pub const DOTNS_TLDS: &\[&str\] = &\[([^\]]*)\];/.exec(
      dotnsName,
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
        "../../../../../rust/crates/truapi-provider/src/networks.rs",
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

describe("hostConnectionTypes", () => {
  test("restricts to secure connections only in production", () => {
    expect(hostConnectionTypes(false)).toEqual({
      secure: true,
      localhost: false,
      unsecure: false,
    });
  });

  test("allows localhost and unsecure connections on dev servers", () => {
    expect(hostConnectionTypes(true)).toEqual({
      secure: true,
      localhost: true,
      unsecure: true,
    });
  });
});

describe("chainBackends", () => {
  // The relay stays on the light client for products that ask for it; the
  // parachains go to their RPC nodes, which the light client could not use
  // for every call (Asset Hub PGAS claims) and which hold the real statement
  // store on People.
  test("defaults the parachains to RPC and the relay to the light client", () => {
    expect(chainBackends(NETWORKS.paseo)).toEqual({
      relay: "light-client",
      assetHub: "rpc",
      people: "rpc",
      bulletin: "rpc",
    });
  });

  test("lets an override choose a chain's backend", () => {
    expect(
      chainBackends(NETWORKS.paseo, { people: "light-client", relay: "rpc" }),
    ).toEqual({
      relay: "rpc",
      assetHub: "rpc",
      people: "light-client",
      bulletin: "rpc",
    });
  });
});
