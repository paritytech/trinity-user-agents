import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import type { Descriptor } from "@parity/trinity-stack";
import {
  BUILT_IN_STACKS,
  DEFAULT_NETWORK_CONFIG,
  NETWORKS,
  StackRefusedError,
  builtInStack,
  bundledCopy,
  chainBackends,
  customPolicy,
  customStackId,
  networkConfig,
  networkConfigFrom,
  type NetworkConfig,
} from "../../../src/platform/network-config.js";
import {
  hostConnectionTypes,
  lightClientChains,
} from "../../../src/platform/network.js";

const repoFile = (path: string) =>
  new URL(`../../../../../${path}`, import.meta.url);

/** What a config says, without the descriptor copy it was read from. */
const read = ({ stack: _stack, ...config }: NetworkConfig) => config;

describe("NETWORKS", () => {
  test("paseo is Paseo Next v2 on the paseo TLD, as its descriptor says", () => {
    expect(read(NETWORKS.paseo)).toEqual({
      id: "paseo",
      displayName: "Paseo Next v2",
      origin: "built-in",
      catalogNetwork: "paseo-next-v2",
      chainInfoNetwork: "paseo",
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
      genesis: {
        relay:
          "0x374057be67b355151f271ff70c3db98308c62c8adc48dc6724b6a009a1a014fd",
        assetHub:
          "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a",
        people:
          "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
        bulletin:
          "0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22",
      },
      dotns: {
        contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
        contenthashSlot: 0,
        textSlot: 1,
        contentGateway: "https://paseo-bulletin-next-ipfs.polkadot.io",
      },
      identityBackend: "https://identity.dotspark.app/",
      bestBlockPosting: {
        peopleGenesis:
          "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
      },
      backup: true,
    });
  });

  test("previewnet is PreviewNet on the testnet TLD, as its descriptor says", () => {
    expect(read(NETWORKS.previewnet)).toEqual({
      id: "previewnet",
      displayName: "Previewnet",
      origin: "built-in",
      catalogNetwork: "previewnet",
      chainInfoNetwork: "paseo",
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
      genesis: {
        relay:
          "0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92",
        assetHub:
          "0xbac97e23fc8f4bccae72a98f8aeb2bcab20bf755862304e4b46ad6473456e896",
        people:
          "0x55e3e689ecfa9d2fffcf7d309b8011956671493982230bfd0420c683542249e9",
        bulletin:
          "0xa081192b90c1f6a3f8e9ce7b2a8246f41af805c66456c84e05fd97c2b3502425",
      },
      dotns: {
        contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
        contenthashSlot: 0,
        textSlot: 1,
        contentGateway: "https://previewnet.substrate.dev",
      },
      identityBackend: "https://previewnet.substrate.dev/dub/",
      bestBlockPosting: null,
      backup: true,
    });
  });

  // PCF's devnet runs on Paseo's chains, none of them in the provider's
  // catalog under that role, so every chain runs on its RPC node.
  test("pcf-devnet is PCF Devnet on the dot TLD, every chain on RPC", () => {
    expect(read(NETWORKS["pcf-devnet"])).toEqual({
      id: "pcf-devnet",
      displayName: "PCF Devnet",
      origin: "built-in",
      catalogNetwork: null,
      chainInfoNetwork: "paseo",
      networkSuffix: "dot",
      webGateway: "dev-dot.li",
      chains: {
        relay: { rpc: "wss://paseo-rpc.n.dwellir.com", backend: "rpc" },
        assetHub: {
          rpc: "wss://asset-hub-paseo-rpc.n.dwellir.com",
          backend: "rpc",
        },
        people: {
          rpc: "wss://people-devnet-rpc.api.polkadotcommunity.foundation",
          backend: "rpc",
        },
        bulletin: { rpc: "wss://bullet.sik.rocks", backend: "rpc" },
      },
      genesis: {
        relay:
          "0x374057be67b355151f271ff70c3db98308c62c8adc48dc6724b6a009a1a014fd",
        assetHub:
          "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
        people:
          "0xe6c30d6e148f250b887105237bcaa5cb9f16dd203bf7b5b9d4f1da7387cb86ec",
        bulletin:
          "0xe101f0fa4627d29a257645e02be86d80378fea1a2bf8fa6a918d150ebc760a59",
      },
      dotns: {
        contentResolver: "7e75491ecfb04900eb05ee63caba2b33900aabb5",
        contenthashSlot: 0,
        textSlot: 1,
        contentGateway: "https://devnet-ipfs.api.polkadotcommunity.foundation",
      },
      identityBackend: "https://polkadot-app.api.polkadotcommunity.foundation/",
      bestBlockPosting: null,
      backup: true,
    });
  });

  // The bundled copies are the CLI's, which scripts/update-stacks.sh refreshes,
  // and the core will verify the very bytes it is handed.
  test("each built-in bundles the CLI's signed copy and link, byte for byte", () => {
    for (const [id, file] of [
      ["paseo", "paseo-next-v2"],
      ["previewnet", "previewnet"],
      ["pcf-devnet", "pcf-devnet"],
    ] as const) {
      const stacks = "rust/crates/truapi-host-cli/stacks";
      expect(BUILT_IN_STACKS[id].bundled).toEqual(
        new Uint8Array(readFileSync(repoFile(`${stacks}/${file}.json`))),
      );
      expect(BUILT_IN_STACKS[id].pointer).toBe(
        readFileSync(repoFile(`${stacks}/${file}.link`), "utf8").trim(),
      );
      expect(NETWORKS[id].stack.document).toBe(BUILT_IN_STACKS[id].bundled);
    }
  });

  // The light client starts from the provider's catalog, and main.ts decides
  // which chains it can serve by comparing a copy with the bundled one.
  test("each bundled copy has the chains the provider catalog holds", () => {
    const catalog = readFileSync(
      repoFile("rust/crates/truapi-provider/src/networks.rs"),
      "utf8",
    );
    for (const network of Object.values(NETWORKS)) {
      if (network.catalogNetwork === null) continue;
      const block = catalog.slice(
        catalog.indexOf(`name: "${network.catalogNetwork}"`),
      );
      const hashes = [...block.matchAll(/genesis_hex: "(0x[0-9a-f]{64})"/g)]
        .slice(0, 4)
        .map((match) => match[1]);
      const { relay, assetHub, bulletin, people } = network.genesis;
      expect(hashes).toEqual([relay, assetHub, bulletin, people]);
    }
  });

  // The resolver contract is the same CREATE3 deployment on Parity's two
  // testnets, so a copy that drifts apart is a mistake. PCF deploys its own.
  test("Paseo Next v2 and PreviewNet read the same DotNS resolver contract", () => {
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
      if (network.catalogNetwork !== null)
        expect(names).toContain(network.catalogNetwork);
  });
});

/** The bundled Paseo copy with its descriptor changed by `edit`, unsigned: the host's rules read only the descriptor. */
function paseoWith(edit: (descriptor: Descriptor) => void): NetworkConfig {
  const copy = bundledCopy(BUILT_IN_STACKS.paseo);
  const descriptor = structuredClone(copy.descriptor);
  edit(descriptor);
  return networkConfigFrom(BUILT_IN_STACKS.paseo, { ...copy, descriptor });
}

describe("networkConfigFrom", () => {
  // Every role is reached by its genesis hash, and the host connects all four.
  test("refuses a descriptor without a chain the host connects", () => {
    expect(() =>
      paseoWith((descriptor) => {
        descriptor.chains = descriptor.chains.filter(
          (chain) => chain.role !== "Bulletin",
        );
      }),
    ).toThrow(
      new StackRefusedError(
        "The Paseo Next v2 stack descriptor (sequence 1793615287) has no Bulletin chain.",
      ),
    );
  });

  // The provider routes by genesis hash, so a shared hash would send one role's calls to another chain.
  test("refuses two chains with one genesis hash", () => {
    expect(() =>
      paseoWith((descriptor) => {
        descriptor.chains[1].genesisHash = descriptor.chains[2].genesisHash;
      }),
    ).toThrow("gives two chains one genesis hash");
  });

  // Content URLs are built as /ipfs/<cid> on the gateway's origin, which would drop a deeper path.
  test("refuses an IPFS gateway that does not serve at /ipfs/", () => {
    expect(() =>
      paseoWith((descriptor) => {
        descriptor.services.find(
          (service) => service.kind === "ipfs-gateway",
        )!.urls = ["https://gateway.example/bulletin/ipfs/"];
      }),
    ).toThrow("does not serve at /ipfs/");
  });

  test("refuses a descriptor without a DotNS content resolver", () => {
    expect(() =>
      paseoWith((descriptor) => {
        delete descriptor.dotns!.contentResolver;
      }),
    ).toThrow("names no DotNS content resolver");
  });

  test("reads the endpoints from the descriptor, and a missing identity backend as none", () => {
    const config = paseoWith((descriptor) => {
      descriptor.chains[2].rpc = [
        "wss://people.example",
        "wss://other.example",
      ];
      descriptor.services = descriptor.services.filter(
        (service) => service.kind !== "dub",
      );
    });
    expect([config.chains.people.rpc, config.identityBackend]).toEqual([
      "wss://people.example",
      null,
    ]);
  });
});

describe("networkConfigFrom: reused values and TLDs", () => {
  const custom = (edit: (descriptor: Descriptor) => void) => {
    const copy = bundledCopy(BUILT_IN_STACKS.paseo);
    const descriptor = structuredClone(copy.descriptor);
    edit(descriptor);
    return networkConfigFrom(customPolicy(copy.pointer), {
      ...copy,
      descriptor,
    });
  };

  // The TLD only names where identities derive; the chains decide where a signature is valid.
  test("lets a custom stack use the dot TLD", () => {
    expect(
      custom((descriptor) => {
        descriptor.tld = "dot";
      }).networkSuffix,
    ).toBe("dot");
  });

  // A development host runs forks and mirrors, so a custom stack may reuse any chain.
  test("lets a custom stack use Polkadot's genesis hashes", () => {
    const polkadotAssetHub =
      "0x68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f";
    expect(
      custom((descriptor) => {
        descriptor.chains[1].genesisHash = polkadotAssetHub;
      }).genesis.assetHub,
    ).toBe(polkadotAssetHub);
  });

  // The core derives identities only under the TLDs it lists.
  test("refuses any stack on a TLD the core does not serve", () => {
    expect(() =>
      paseoWith((descriptor) => {
        descriptor.tld = "kusama";
      }),
    ).toThrow(
      "uses the TLD kusama; this host's core derives identities only under dot, paseo, testnet",
    );
  });

  // A custom stack's namespace is its identity, never a name its operator picks.
  test("keys a custom stack by its signer, or by its URL when unsigned", () => {
    const config = custom(() => {});
    expect([
      config.id,
      config.origin,
      config.backup,
      config.catalogNetwork,
    ]).toEqual([
      customStackId(bundledCopy(BUILT_IN_STACKS.paseo).pointer),
      "custom",
      false,
      null,
    ]);
    expect(config.id).toMatch(/^custom-[0-9a-f]{16}$/);
    expect(customStackId({ url: "https://a.example/s.json" })).not.toBe(
      customStackId({ url: "https://b.example/s.json" }),
    );
    expect(
      customStackId({
        url: "https://a.example/s.json",
        key: `0x${"11".repeat(32)}`,
      }),
    ).toBe(
      customStackId({
        url: "https://b.example/s.json",
        key: `0x${"11".repeat(32)}`,
      }),
    );
  });
});

describe("lightClientChains", () => {
  test("serves every chain whose catalog genesis is the copy's", () => {
    expect(lightClientChains(NETWORKS.paseo, NETWORKS.paseo.genesis)).toEqual(
      new Set(["relay", "assetHub", "people", "bulletin"]),
    );
  });

  // A stack that restarted after this build has chains the catalog does not know.
  test("serves no chain the catalog holds under another genesis", () => {
    const restarted = paseoWith((descriptor) => {
      descriptor.chains[2].genesisHash = `0x${"11".repeat(32)}`;
    });
    expect(lightClientChains(restarted, NETWORKS.paseo.genesis)).toEqual(
      new Set(["relay", "assetHub", "bulletin"]),
    );
  });

  // Parachains on the light client follow the catalog's relay.
  test("serves nothing when the relay restarted", () => {
    const restarted = paseoWith((descriptor) => {
      descriptor.chains[0].genesisHash = `0x${"11".repeat(32)}`;
    });
    expect(lightClientChains(restarted, NETWORKS.paseo.genesis).size).toBe(0);
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

  test("names the built-in stack the same way", () => {
    expect(builtInStack()).toBe(BUILT_IN_STACKS.paseo);
    expect(builtInStack(" previewnet ")).toBe(BUILT_IN_STACKS.previewnet);
    expect(() => builtInStack("preview")).toThrow("Unknown network");
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
