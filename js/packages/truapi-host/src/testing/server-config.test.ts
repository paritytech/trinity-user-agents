import { describe, expect, it } from "bun:test";
import { hostPageUrl } from "./host-page-url.js";
import { createTestHostServer } from "./server.js";

describe("createTestHostServer host configuration", () => {
  it("returns a URL carrying the configuration it was given", async () => {
    // A suite that starts the server directly, rather than through the
    // Playwright fixture, gets a ready-to-open URL -- that is the shape
    // `@parity/host-api-test-sdk`'s server has, and what a migrating suite
    // that never used the fixture depends on.
    const server = await createTestHostServer({
      unref: true,
      productUrl: "http://localhost:5173",
      productId: "localhost:5173",
      accounts: ["alice", "bob"],
    });
    const url = new URL(server.url);
    expect(url.searchParams.get("product")).toBe("http://localhost:5173");
    expect(url.searchParams.get("productId")).toBe("localhost:5173");
    expect(url.searchParams.get("accounts")).toBe("alice,bob");
    await server.close();
  });

  it("returns a bare base when given no configuration", async () => {
    // The fixture appends its own per-test configuration, so an unconfigured
    // server must not carry a half-built query string into that.
    const server = await createTestHostServer({ unref: true });
    expect(new URL(server.url).search).toBe("");
    await server.close();
  });

  it("serves the chains `networks` names, expanded as the fixture expands them", async () => {
    // A suite that starts the server itself gets its chains from here, so
    // they have to reach the page as the fixture would send them.
    const genesisHash = `0x${"aa".repeat(32)}`;
    const server = await createTestHostServer({
      unref: true,
      productUrl: "http://localhost:5173",
      networks: [
        { id: "paseo-asset-hub", genesisHash, rpcUrl: "wss://hub.invalid" },
      ],
    });
    const url = new URL(server.url);
    expect(JSON.parse(url.searchParams.get("mock")!)).toMatchObject({
      chainProxies: [{ rpcUrl: "wss://hub.invalid" }],
      supportedChains: {
        network: "paseo",
        chains: [{ identifier: "AssetHub", genesisHash }],
      },
    });
    expect(JSON.parse(url.searchParams.get("runtimeConfig")!)).toEqual({
      assetHub: { genesisHash },
    });
    await server.close();
  });

  it("lets explicit mock settings win over what `networks` derives", async () => {
    const server = await createTestHostServer({
      unref: true,
      productUrl: "http://localhost:5173",
      networks: [
        {
          id: "paseo-asset-hub",
          genesisHash: `0x${"aa".repeat(32)}`,
          rpcUrl: "wss://hub.invalid",
        },
      ],
      mock: { chainProxies: [] },
    });
    expect(
      JSON.parse(new URL(server.url).searchParams.get("mock")!),
    ).toMatchObject({
      chainProxies: [],
      supportedChains: { network: "paseo" },
    });
    await server.close();
  });

  it("refuses `chain` and `networks` together", async () => {
    const chain = {
      id: "paseo-asset-hub",
      genesisHash: `0x${"aa".repeat(32)}`,
      rpcUrl: "wss://hub.invalid",
    };
    await expect(
      createTestHostServer({
        unref: true,
        productUrl: "http://localhost:5173",
        chain,
        networks: [chain],
      }),
    ).rejects.toThrow(/either `chain`/);
  });

  it("refuses chains it could not apply, rather than dropping them", async () => {
    // Without a `productUrl` the server returns a bare base the fixture
    // configures per test, so chains given here would reach no page.
    await expect(
      createTestHostServer({
        unref: true,
        networks: [
          {
            id: "paseo-asset-hub",
            genesisHash: `0x${"aa".repeat(32)}`,
            rpcUrl: "wss://hub.invalid",
          },
        ],
      }),
    ).rejects.toThrow(/`networks` needs `productUrl`/);
  });

  it("accepts settings that ask for nothing without a productUrl", async () => {
    const server = await createTestHostServer({
      unref: true,
      loopbackStatements: false,
      networks: [],
    });
    expect(new URL(server.url).search).toBe("");
    await server.close();
  });

  it("rejects a derivation-path account, and says why", async () => {
    await expect(
      createTestHostServer({
        unref: true,
        productUrl: "http://localhost:5173",
        accounts: [
          { name: "alice", uri: "//Alice" } as unknown as {
            name: string;
            entropy: Uint8Array;
          },
        ],
      }),
    ).rejects.toThrow(/`uri` is not supported/);
  });

  it("rejects a pinned product account, and says why", async () => {
    await expect(
      createTestHostServer({
        unref: true,
        productUrl: "http://localhost:5173",
        productAccounts: { "demo.dot/0": "bob" },
      }),
    ).rejects.toThrow(/DERIVED from \(session root, product id\)/);
  });

  it("builds the same URL for both entry points", () => {
    // The fixture and the server share one builder precisely so an option
    // cannot be taught to one and forgotten by the other.
    const config = {
      productUrl: "http://localhost:5173",
      accounts: ["alice"],
      loginBehavior: "manual" as const,
    };
    expect(hostPageUrl("http://127.0.0.1:1234", config)).toBe(
      hostPageUrl("http://127.0.0.1:1234", config),
    );
    const url = new URL(hostPageUrl("http://127.0.0.1:1234", config));
    expect(url.searchParams.get("login")).toBe("manual");
  });
});

describe("host page URL", () => {
  it("carries boot-time product storage", () => {
    const url = new URL(
      hostPageUrl("http://127.0.0.1:1234", {
        productUrl: "http://localhost:5173",
        productStorage: { "app:state": "{}" },
      }),
    );
    expect(JSON.parse(url.searchParams.get("storage")!)).toEqual({
      "app:state": "{}",
    });
  });
});

describe("diagnostic options reach the page", () => {
  it("carries topology and logLevel through the URL", () => {
    const url = new URL(
      hostPageUrl("http://host.test/", {
        productUrl: "http://localhost:5173",
        topology: "main-thread",
        logLevel: "debug",
      }),
    );
    expect(url.searchParams.get("topology")).toBe("main-thread");
    expect(url.searchParams.get("logLevel")).toBe("debug");
  });

  it("omits them when unset, so the page keeps its production defaults", () => {
    const url = new URL(
      hostPageUrl("http://host.test/", {
        productUrl: "http://localhost:5173",
      }),
    );
    expect(url.searchParams.has("topology")).toBe(false);
    expect(url.searchParams.has("logLevel")).toBe(false);
  });

  it("serves them from the server entry point too", async () => {
    // Both entry points build the same query string. Asserting only through
    // the fixture would let the server keep an option the page never sees.
    const server = await createTestHostServer({
      unref: true,
      productUrl: "http://localhost:5173",
      topology: "main-thread",
      logLevel: "trace",
    });
    const url = new URL(server.url);
    expect(url.searchParams.get("topology")).toBe("main-thread");
    expect(url.searchParams.get("logLevel")).toBe("trace");
    await server.close();
  });
});

describe("signing as an identity that is not a built-in", () => {
  const entropy = Uint8Array.from({ length: 32 }, (_, i) => i + 1);

  it("carries explicit entropy through the URL, hex-encoded", () => {
    const url = new URL(
      hostPageUrl("http://host.test/", {
        productUrl: "http://localhost:5173",
        accounts: ["alice", { name: "enrolled", entropy }],
      }),
    );
    const value = url.searchParams.get("accounts") ?? "";
    const [first, second] = value.split(",");
    expect(first).toBe("alice");
    // A built-in stays a bare name; entropy rides as `name:<64 hex>`, which is
    // what lets a suite sign as a personhood-enrolled account.
    expect(second).toBe(
      `enrolled:${[...entropy].map((b) => b.toString(16).padStart(2, "0")).join("")}`,
    );
    expect(second?.split(":")[1]).toHaveLength(64);
  });

  it("round-trips the bytes, so the host activates the intended session", () => {
    const url = new URL(
      hostPageUrl("http://host.test/", {
        productUrl: "http://localhost:5173",
        accounts: [{ name: "enrolled", entropy }],
      }),
    );
    const encoded =
      (url.searchParams.get("accounts") ?? "").split(":")[1] ?? "";
    const decoded = Uint8Array.from(
      (encoded.match(/../g) ?? []).map((b) => parseInt(b, 16)),
    );
    expect([...decoded]).toEqual([...entropy]);
  });
});

describe("how resource allocation is answered", () => {
  it("defaults to granting without performing, and says so only when asked", () => {
    // Absent from the URL means the page default applies. Sending it always
    // would make a later change to that default invisible here.
    const unset = new URL(
      hostPageUrl("http://host.test/", { productUrl: "http://p.test" }),
    );
    expect(unset.searchParams.has("allowances")).toBe(false);

    const chain = new URL(
      hostPageUrl("http://host.test/", {
        productUrl: "http://p.test",
        allowances: "chain",
      }),
    );
    expect(chain.searchParams.get("allowances")).toBe("chain");
  });

  it("carries the granted mode explicitly when a suite pins it", () => {
    const url = new URL(
      hostPageUrl("http://host.test/", {
        productUrl: "http://p.test",
        allowances: "granted",
      }),
    );
    expect(url.searchParams.get("allowances")).toBe("granted");
  });
});
