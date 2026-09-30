import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  effectiveProductId,
  effectiveProductIdFor,
  KNOWN_DOTNS_TLDS,
  normalizeAddress,
  parseAddress,
} from "./address.js";
import { parseProductUrl } from "./product.js";

describe("normalizeAddress", () => {
  test("adds http to a bare host and port", () => {
    expect(normalizeAddress(" localhost:3000/app ")).toBe(
      "http://localhost:3000/app",
    );
    expect(normalizeAddress("192.168.0.7:3000")).toBe(
      "http://192.168.0.7:3000",
    );
  });

  test("keeps an explicit scheme", () => {
    expect(normalizeAddress("https://example.test/")).toBe(
      "https://example.test/",
    );
  });

  // A typed `javascript:` or `data:` address must still be refused, not
  // rescued by the added scheme.
  test("does not turn a script scheme into a URL", () => {
    expect(() =>
      parseProductUrl(normalizeAddress("javascript:alert(1)")),
    ).toThrow();
    expect(() =>
      parseProductUrl(normalizeAddress("data:text/html,x")),
    ).toThrow();
  });

  test("leaves empty text empty", () => {
    expect(normalizeAddress("  ")).toBe("");
  });
});

describe("effectiveProductId", () => {
  test("derives a localhost id that keeps the port", () => {
    expect(effectiveProductId(new URL("http://localhost:3000/"), "")).toEqual({
      id: "localhost:3000",
      source: "derived",
      usable: true,
    });
  });

  // The core refuses an IP address as a product id, so the host must say an id
  // has to be entered instead of offering one that will fail.
  test("marks a bare LAN address as needing an entered id", () => {
    expect(effectiveProductId(new URL("http://192.168.0.7:3000/"), "")).toEqual(
      {
        id: "192.168.0.7",
        source: "derived",
        usable: false,
      },
    );
  });

  test("uses an entered id for any address", () => {
    expect(
      effectiveProductId(new URL("http://192.168.0.7:3000/"), " myapp.paseo "),
    ).toEqual({ id: "myapp.paseo", source: "entered", usable: true });
    expect(
      effectiveProductId(new URL("http://localhost:3000/"), "myapp.paseo").id,
    ).toBe("myapp.paseo");
  });

  // Editing the address must not silently reset a chosen id.
  test("keeps the entered id when the address changes", () => {
    for (const address of [
      "http://localhost:3001/",
      "http://192.168.0.94:3000/",
    ])
      expect(effectiveProductId(new URL(address), "myapp.paseo").id).toBe(
        "myapp.paseo",
      );
  });
});

describe("parseAddress: product names", () => {
  test("reads a bare name as a dotNS product name", () => {
    expect(parseAddress("chat-spa-probe.paseo")).toEqual({
      kind: "name",
      name: "chat-spa-probe.paseo",
      suffix: "",
    });
  });

  // The address bar used to rewrite a typed name into http://name/, and that
  // text is what people press Enter on next. It means the same name.
  test("reads the http:// form the bar used to produce as the same name", () => {
    expect(parseAddress("http://chat-spa-probe.paseo/")).toEqual(
      parseAddress("chat-spa-probe.paseo"),
    );
    expect(parseAddress("https://Chat-Spa-Probe.PASEO")).toEqual(
      parseAddress("chat-spa-probe.paseo"),
    );
  });

  test("reads a polkadot:// name and keeps path, query and hash", () => {
    expect(parseAddress("polkadot://myapp.paseo/a/b?x=1#top")).toEqual({
      kind: "name",
      name: "myapp.paseo",
      suffix: "a/b?x=1#top",
    });
  });

  test("gives a name its own id, and an entered id still wins", () => {
    const address = parseAddress("myapp.paseo");
    expect(effectiveProductIdFor(address, "")).toEqual({
      id: "myapp.paseo",
      source: "derived",
      usable: true,
    });
    expect(effectiveProductIdFor(address, " other.paseo ")).toEqual({
      id: "other.paseo",
      source: "entered",
      usable: true,
    });
  });

  // .dot and .testnet are real dotNS TLDs on other networks. Opening them as
  // web addresses would hide that this host cannot resolve them.
  test("explains a name on another network", () => {
    expect(() => parseAddress("myapp.dot")).toThrow("another network");
    expect(() => parseAddress("myapp.testnet")).toThrow("Paseo");
  });

  test("refuses a name with a port or credentials", () => {
    expect(() => parseAddress("myapp.paseo:8080")).toThrow("no port");
    expect(() => parseAddress("user@myapp.paseo")).toThrow("no port");
  });

  test("refuses names that are not one label and the TLD", () => {
    expect(() => parseAddress("app.myapp.paseo")).toThrow("one label");
    expect(() => parseAddress("-bad.paseo")).toThrow("not a product name");
    expect(() => parseAddress("bad_name.paseo")).toThrow("not a product name");
  });

  test("points a public gateway address back at the name", () => {
    expect(() => parseAddress("https://chat-spa-probe.paseo.li/")).toThrow(
      "Type chat-spa-probe.paseo instead",
    );
  });

  test("refuses a polkadot:// address that is not a name", () => {
    expect(() => parseAddress("polkadot://example.com")).toThrow("dotNS name");
  });
});

describe("parseAddress: web addresses keep their meaning", () => {
  test("opens localhost, a LAN address and an ordinary site as URLs", () => {
    expect(parseAddress("localhost:3000")).toEqual({
      kind: "url",
      url: new URL("http://localhost:3000"),
    });
    expect(parseAddress("192.168.0.7:3000/app").kind).toBe("url");
    expect(parseAddress("https://example.test/x").kind).toBe("url");
  });

  // A host name that only looks similar to a dotNS TLD is an ordinary host.
  test("does not take a lookalike host for a name", () => {
    expect(parseAddress("paseo.example.com").kind).toBe("url");
    expect(parseAddress("macbook.local").kind).toBe("url");
    expect(parseAddress("mypaseo").kind).toBe("url");
  });

  test("still refuses script schemes", () => {
    expect(() => parseAddress("javascript:alert(1)")).toThrow();
  });
});

describe("dotNS TLDs", () => {
  // The core decides which TLDs are dotNS names. This host copies the list, so
  // a TLD added or removed there must be added or removed here.
  test("match the Rust core's DOTNS_TLDS", () => {
    const platform = readFileSync(
      new URL("../../../rust/crates/truapi/src/platform.rs", import.meta.url),
      "utf8",
    );
    const list = /pub const DOTNS_TLDS: &\[&str\] = &\[([^\]]*)\];/.exec(
      platform,
    );
    const core = [...(list?.[1] ?? "").matchAll(/"([a-z]+)"/g)].map(
      (match) => match[1],
    );
    expect(KNOWN_DOTNS_TLDS).toEqual(core);
  });
});
