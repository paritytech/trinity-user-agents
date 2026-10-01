import { describe, expect, test } from "bun:test";
import {
  hostBase,
  loaderUrl,
  mountScope,
  parseMountScope,
  productLabel,
  requireSecureHost,
  walletKey,
} from "./mount.js";

const CID = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";
const mount = {
  walletId: "wallet-one",
  productId: "chat.paseo",
  cid: CID,
  container: true,
};

describe("hostBase", () => {
  // The same build is served at a project path or at a domain root, and the
  // base is how it learns which.
  test("takes the directory the build was given", () => {
    const page = "https://owner.github.io/repo/";
    expect(hostBase("/repo/", page)).toBe("/repo/");
    expect(hostBase("/", "https://host.example/")).toBe("/");
    expect(hostBase("./", `${page}index.html`)).toBe("/repo/");
    expect(hostBase("/repo", page)).toBe("/repo/");
  });
});

describe("mountScope", () => {
  test("is parsed back to the content and container setting it was built from", () => {
    expect(parseMountScope("/repo/", mountScope("/repo/", mount))).toEqual({
      cid: CID,
      container: true,
    });
    expect(
      parseMountScope("/", mountScope("/", { ...mount, container: false })),
    ).toEqual({ cid: CID, container: false });
  });

  // This is what keeps two tabs from sharing a worker: a different scope is a
  // different registration, so one tab's worker never answers the other's page.
  test("differs by wallet, by product, by content and by container setting", () => {
    const scopes = [
      mount,
      { ...mount, walletId: "wallet-two" },
      { ...mount, productId: "other.paseo" },
      { ...mount, cid: `${CID}x` },
      { ...mount, container: false },
    ].map((variant) => mountScope("/repo/", variant));
    expect(new Set(scopes).size).toBe(scopes.length);
  });

  test("is the same for the same wallet opening the same content again", () => {
    expect(mountScope("/repo/", mount)).toBe(
      mountScope("/repo/", { ...mount }),
    );
  });

  test("does not spell the wallet id out", () => {
    expect(mountScope("/repo/", mount)).not.toContain("wallet-one");
    expect(walletKey("wallet-one")).toMatch(/^[0-9a-f]{12}$/);
  });

  test("names no mount for a path outside the base, or one that is not shaped like a mount", () => {
    const scope = mountScope("/repo/", mount);
    expect(parseMountScope("/other/", scope)).toBeNull();
    expect(parseMountScope("/repo/", "/repo/")).toBeNull();
    expect(parseMountScope("/repo/", "/repo/product/x/y/z/")).toBeNull();
    expect(parseMountScope("/repo/", `${scope}extra/`)).toBeNull();
  });
});

describe("productLabel", () => {
  test("keeps two ids that differ only in punctuation apart", () => {
    expect(productLabel("a.b")).not.toBe(productLabel("a-b"));
    expect(productLabel("A.B")).not.toBe(productLabel("a.b"));
  });

  test("stays within a DNS-label length and a plain character set", () => {
    const label = productLabel("x".repeat(500));
    expect(label.length).toBeLessThanOrEqual(63);
    expect(label).toMatch(/^[a-z0-9-]+$/);
  });
});

describe("loaderUrl", () => {
  test("is a static page under the base, carrying the mount and what to load", () => {
    const scope = mountScope("/repo/", mount);
    const url = loaderUrl({
      base: "/repo/",
      origin: "https://owner.github.io",
      scope,
      gateway: "https://gateway.example",
      kind: "car",
      start: "/a?b#c",
    });
    expect(url.origin + url.pathname).toBe(
      "https://owner.github.io/repo/truapi-sandbox/index.html",
    );
    expect(Object.fromEntries(url.searchParams)).toEqual({
      scope,
      gateway: "https://gateway.example",
      kind: "car",
      start: "/a?b#c",
    });
  });
});

describe("requireSecureHost", () => {
  test("refuses a page that is not a secure context", () => {
    expect(() => requireSecureHost(false, "http://192.168.0.7:5180")).toThrow(
      "secure",
    );
    expect(() => requireSecureHost(true, "https://x")).not.toThrow();
  });
});
