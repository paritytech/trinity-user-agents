import { describe, expect, test } from "bun:test";
import {
  type AssetKind,
  type AssetRequest,
  classifyAssetPath,
  decideAsset,
} from "./access.js";
import { productLabel } from "./origin.js";
import type { SandboxPolicy } from "./policy.js";

const label = productLabel("a.paseo");
const auto: SandboxPolicy = { template: "", hostOrigins: [] };
const range: SandboxPolicy = {
  template: "https://host.example:{9450-9452}",
  hostOrigins: [],
};

const ask = (
  policy: SandboxPolicy,
  host: string | undefined,
  change: Partial<AssetRequest> = {},
) =>
  decideAsset(policy, {
    kind: "loader",
    method: "GET",
    host,
    fetchDest: "iframe",
    secure: false,
    ...change,
  });

describe("decideAsset", () => {
  test("serves a product origin its loader into a frame, and names who may embed it", () => {
    expect(ask(auto, `${label}.localhost:5180`)).toEqual({
      type: "serve",
      frameAncestors: ["http://localhost:5180", "http://127.0.0.1:5180"],
    });
    expect(ask(range, "host.example:9451", { secure: true })).toEqual({
      type: "serve",
      frameAncestors: ["https://host.example"],
    });
  });

  // The wallets live on these origins. A crafted link must not get the loader
  // there, so the server does not send it.
  test("refuses every sandbox file on the wallet's origin but the worker", () => {
    for (const [policy, host] of [
      [auto, "localhost:5180"],
      [auto, "127.0.0.1:5180"],
      [auto, "wallet.localhost:5180"],
      [range, "host.example"],
      [range, "host.example:443"],
      [range, "other.example:9451"],
    ] as const)
      for (const kind of ["loader", "script"] as const)
        expect(ask(policy, host, { kind })).toMatchObject({
          type: "refuse",
          status: 404,
        });
  });

  // The worker there is the one that removes itself, so it is what a worker an
  // earlier build left on the wallet's origin is replaced with.
  test("answers the wallet origin's worker update with the worker, which retires there", () => {
    expect(ask(auto, "localhost:5180", { kind: "worker" })).toEqual({
      type: "serve",
    });
  });

  test("refuses the loader unless the browser is loading it into a frame", () => {
    for (const fetchDest of [undefined, "document", "script", "iframe2"])
      expect(ask(auto, `${label}.localhost:5180`, { fetchDest })).toMatchObject(
        {
          type: "refuse",
          status: 403,
        },
      );
  });

  test("serves scripts and the worker to product origins whatever they are loaded for", () => {
    for (const kind of ["script", "worker"] as const)
      expect(
        ask(auto, `${label}.localhost:5180`, { kind, fetchDest: "script" }),
      ).toEqual({ type: "serve" });
  });

  test("refuses the loader when no host is configured to embed it", () => {
    expect(
      ask(
        { template: "https://{label}.sandbox.example", hostOrigins: [] },
        `${label}.sandbox.example`,
      ),
    ).toMatchObject({ type: "refuse", status: 403 });
  });

  test("refuses other methods and a Host header it cannot read", () => {
    expect(
      ask(auto, `${label}.localhost:5180`, { method: "POST" }),
    ).toMatchObject({
      type: "refuse",
      status: 405,
    });
    for (const host of [undefined, "", "a b", "host:99999x"])
      expect(ask(auto, host)).toMatchObject({ type: "refuse", status: 400 });
  });
});

describe("classifyAssetPath", () => {
  const known: Record<string, AssetKind> = {
    "/__sandbox/index.html": "loader",
    "/__sandbox/page.js": "script",
    "/__sandbox-sw.js": "worker",
  };

  test("names the plain paths", () => {
    expect(classifyAssetPath("/__sandbox/index.html", known)).toBe("loader");
    expect(classifyAssetPath("/__sandbox-sw.js", known)).toBe("worker");
  });

  // A static server decodes and normalises before it opens a file, so each of
  // these can reach a sandbox file. None may pass as an ordinary path.
  test("reserves every other spelling of the sandbox's prefix", () => {
    for (const path of [
      "/__sandbox/%69ndex.html",
      "/__sandbox/%70age.js",
      "/%5f%5fsandbox/page.js",
      "/__SANDBOX/page.js",
      "/__sandbox/",
      "/__sandbox",
      "//__sandbox/page.js",
      "/x/../__sandbox/page.js",
      "/x/%2e%2e/__sandbox/page.js",
      "/__sandbox/%2e/page.js",
      "/__sandbox/%2570age.js",
      "/__sandbox\\page.js",
      "/__sandbox-SW.js",
      "/__sandbox-sw.js/",
      "/__sandbox/%zz",
    ])
      expect({ path, kind: classifyAssetPath(path, known) }).toEqual({
        path,
        kind: "reserved",
      });
  });

  test("leaves a product's own paths alone", () => {
    for (const path of [
      "/",
      "/index.html",
      "/assets/app.js",
      "/sandbox/a.js",
      "/a%20b",
      "/_sandbox/x",
    ])
      expect(classifyAssetPath(path, known)).toBeNull();
  });
});
