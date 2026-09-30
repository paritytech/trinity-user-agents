import { describe, expect, test } from "bun:test";
import { parseSandboxPolicy, productLabel } from "./origin.js";
import {
  type EmbeddingContext,
  type SandboxPolicy,
  authorizeEmbedding,
  identifySandbox,
  trustedHostOrigins,
} from "./policy.js";

const parts = (url: string) => {
  const { protocol, hostname, port } = new URL(url);
  return { protocol, hostname, port };
};
const label = productLabel("a.paseo");
const other = productLabel("b.paseo");

const auto: SandboxPolicy = { template: "", hostOrigins: [] };
const range: SandboxPolicy = {
  template: "https://host.example:{9450-9452}",
  hostOrigins: [],
};
const labelled: SandboxPolicy = {
  template: "https://{label}.sandbox.example",
  hostOrigins: ["https://host.example"],
};

describe("identifySandbox", () => {
  // The wallets sit on `localhost`, so nothing there may count as a product origin.
  test("beside a loopback host, only a product label under .localhost", () => {
    expect(
      identifySandbox(auto, parts(`http://${label}.localhost:5180`)),
    ).toEqual({ label });
    for (const wallet of [
      "http://localhost:5180",
      "http://127.0.0.1:5180",
      "http://wallet.localhost:5180",
      `http://x.${label}.localhost:5180`,
      "http://evil.example:5180",
    ])
      expect(identifySandbox(auto, parts(wallet))).toBeNull();
  });

  test("a label template matches its own shape, scheme and port, and no plain host name", () => {
    expect(
      identifySandbox(labelled, parts(`https://${label}.sandbox.example`)),
    ).toEqual({ label });
    for (const not of [
      "https://sandbox.example",
      "https://wallet.sandbox.example",
      "https://host.example",
      `http://${label}.sandbox.example`,
      `https://${label}.sandbox.example:8443`,
      `https://${label}.sandbox.example.evil.example`,
      `https://${label}.other.example`,
    ])
      expect(identifySandbox(labelled, parts(not))).toBeNull();
  });

  // The wallets and the products share one name; only the port tells them apart.
  test("a port range matches its own name and ports, not the default port", () => {
    for (const port of [9450, 9451, 9452])
      expect(
        identifySandbox(range, parts(`https://host.example:${port}`)),
      ).not.toBeNull();
    for (const not of [
      "https://host.example",
      "https://host.example:443",
      "https://host.example:9453",
      "https://host.example:9449",
      "https://other.example:9450",
      "http://host.example:9450",
    ])
      expect(identifySandbox(range, parts(not))).toBeNull();
  });

  test("a server that cannot see the scheme still reads the port", () => {
    expect(
      identifySandbox(range, { hostname: "host.example", port: "" }),
    ).toBeNull();
    expect(
      identifySandbox(range, { hostname: "host.example", port: "9451" }),
    ).not.toBeNull();
  });
});

describe("trustedHostOrigins", () => {
  test("beside a loopback host: the loopback names on the product's scheme and port", () => {
    expect(
      trustedHostOrigins(auto, { protocol: "http:", port: "5180" }),
    ).toEqual(["http://localhost:5180", "http://127.0.0.1:5180"]);
  });

  test("a port range puts the wallets on the name's default port", () => {
    expect(
      trustedHostOrigins(range, { protocol: "https:", port: "9450" }),
    ).toEqual(["https://host.example"]);
  });

  test("a label template has none of its own, so it needs them configured", () => {
    expect(
      trustedHostOrigins(
        { ...labelled, hostOrigins: [] },
        { protocol: "https:", port: "" },
      ),
    ).toEqual([]);
    expect(
      trustedHostOrigins(labelled, { protocol: "https:", port: "" }),
    ).toEqual(["https://host.example"]);
  });
});

describe("authorizeEmbedding", () => {
  const embedded = (
    policy: SandboxPolicy,
    at: string,
    change: Partial<EmbeddingContext> = {},
  ) => {
    const { protocol, hostname, port, origin } = new URL(at);
    return () =>
      authorizeEmbedding(policy, {
        location: { protocol, hostname, port, origin },
        topLevel: false,
        ancestorOrigins: ["http://localhost:5180"],
        ...change,
      });
  };
  const product = `http://${label}.localhost:5180/`;

  test("accepts the configured host as the only ancestor", () => {
    expect(embedded(auto, product)()).toEqual({
      hostOrigin: "http://localhost:5180",
      label,
    });
  });

  test("refuses to run on its own, whatever the link says", () => {
    expect(embedded(auto, product, { topLevel: true })).toThrow(
      "never on its own",
    );
  });

  test("refuses the wallet's own origin and any other that is not a product origin", () => {
    for (const at of [
      "http://localhost:5180/",
      "http://127.0.0.1:5180/",
      "http://wallet.localhost:5180/",
    ])
      expect(
        embedded(auto, at, { ancestorOrigins: ["http://localhost:5180"] }),
      ).toThrow("not a product origin");
  });

  test("refuses a parent that is not the host, however the link names the host", () => {
    for (const ancestorOrigins of [
      ["https://evil.example"],
      ["http://localhost:6000"],
      ["https://localhost:5180"],
      [`http://${other}.localhost:5180`],
      [],
    ])
      expect(embedded(auto, product, { ancestorOrigins })).toThrow(
        "not this host",
      );
  });

  test("refuses a host that is itself framed by another page", () => {
    expect(
      embedded(auto, product, {
        ancestorOrigins: ["http://localhost:5180", "https://evil.example"],
      }),
    ).toThrow("not this host");
  });

  // The host's frames send no referrer, so there is nothing to fall back on.
  test("with no ancestor list it refuses, whatever the link says", () => {
    expect(embedded(auto, product, { ancestorOrigins: undefined })).toThrow(
      "location.ancestorOrigins",
    );
  });

  test("a label template with no configured host refuses everything", () => {
    expect(
      embedded(
        { ...labelled, hostOrigins: [] },
        `https://${label}.sandbox.example/`,
        { ancestorOrigins: ["https://host.example"] },
      ),
    ).toThrow("WEB_SIGNING_HOST_ORIGINS");
  });

  test("a port range accepts the name's wallets and no product port", () => {
    const at = "https://host.example:9451/";
    expect(
      embedded(range, at, { ancestorOrigins: ["https://host.example"] })(),
    ).toEqual({
      hostOrigin: "https://host.example",
      label: "",
    });
    expect(
      embedded(range, at, { ancestorOrigins: ["https://host.example:9450"] }),
    ).toThrow("not this host");
  });
});

describe("parseSandboxPolicy", () => {
  test("keeps a valid policy and normalises the host origins", () => {
    expect(
      parseSandboxPolicy(" https://host.example:{9450-9452} ", [
        "https://host.example/",
        "https://host.example",
      ]),
    ).toEqual({
      template: "https://host.example:{9450-9452}",
      hostOrigins: ["https://host.example"],
    });
  });

  // A mistake here would let a product embed the loader as the host.
  test("refuses a host origin that is also a product origin, or not an origin", () => {
    expect(() =>
      parseSandboxPolicy("https://host.example:{9450-9452}", [
        "https://host.example:9451",
      ]),
    ).toThrow("also a product origin");
    for (const bad of [
      "not a url",
      "ftp://host.example",
      "https://host.example/path",
      "https://user@host.example",
      "https://host.example/?q=1",
    ])
      expect(() => parseSandboxPolicy("", [bad])).toThrow();
  });

  test("refuses a template that cannot give each product its own origin", () => {
    for (const bad of [
      "https://sandbox.example",
      "https://host.example:{9452-9450}",
      "https://host.example:{1-500}",
    ])
      expect(() => parseSandboxPolicy(bad, [])).toThrow();
  });
});
