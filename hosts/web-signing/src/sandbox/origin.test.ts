import { describe, expect, test } from "bun:test";
import { PortLedger } from "./ports.js";
import { serialLocks } from "./test-support.js";
import {
  originFromTemplate,
  productLabel,
  containerOffAllowed,
  requireSecureHost,
  sandboxOrigin,
  sandboxUrl,
} from "./origin.js";

const ledger = () => {
  const saved = new Map<string, string>();
  return new PortLedger(
    {
      getItem: (key) => saved.get(key) ?? null,
      setItem: (key, value) => {
        saved.set(key, value);
      },
    },
    serialLocks(),
  );
};

const at = (url: string) => {
  const { protocol, hostname, port, origin } = new URL(url);
  return { protocol, hostname, port, origin };
};

describe("productLabel", () => {
  test("is a DNS label", () => {
    for (const id of [
      "chat-spa-probe.paseo",
      "A.B_C.paseo",
      "..",
      "x".repeat(200),
      "ünïcode.paseo",
    ]) {
      const label = productLabel(id);
      expect(label).toMatch(/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/);
      expect(label.length).toBeLessThanOrEqual(63);
    }
  });

  // Ids that read the same after cleaning must still not share an origin.
  test("never gives two ids one label", () => {
    const ids = [
      "a.b-paseo",
      "a-b.paseo",
      "A.B.paseo",
      "a.b.paseo",
      "a_b.paseo",
      "ab.paseo",
    ];
    expect(new Set(ids.map(productLabel)).size).toBe(ids.length);
  });
});

describe("sandboxOrigin", () => {
  test("gives a loopback host a .localhost origin of its own per product", async () => {
    const host = at("http://127.0.0.1:5180/");
    const a = await sandboxOrigin(host, "a.paseo", {
      template: "",
      ports: ledger(),
    });
    const b = await sandboxOrigin(host, "b.paseo", {
      template: "",
      ports: ledger(),
    });
    expect(
      [a, b].every((origin) =>
        /^http:\/\/[a-z0-9-]+\.localhost:5180$/.test(origin),
      ),
    ).toBe(true);
    expect(a).not.toBe(b);
    expect(a).not.toBe(host.origin);
  });

  test("refuses a host with no such origin instead of falling back to its own", async () => {
    await expect(
      sandboxOrigin(at("http://192.168.0.7:5180/"), "a.paseo", {
        template: "",
        ports: ledger(),
      }),
    ).rejects.toThrow("No sandbox origin");
  });

  test("uses a template when one is set, on any host", async () => {
    const origin = await sandboxOrigin(at("https://host.example/"), "a.paseo", {
      template: "https://{label}.sandbox.example",
      ports: ledger(),
    });
    expect(origin).toBe(`https://${productLabel("a.paseo")}.sandbox.example`);
  });

  test("refuses a template that lands on the host's own origin", async () => {
    const host = at("https://host.example/");
    await expect(
      sandboxOrigin(host, "a.paseo", {
        template: "https://host.example",
        ports: ledger(),
      }),
    ).rejects.toThrow();
    expect(() =>
      originFromTemplate("https://host.example/{label}", "zzq"),
    ).toThrow("{label} in its host");
  });
});

describe("originFromTemplate", () => {
  test("wants {label} in the host, an http(s) origin and nothing after it", () => {
    for (const bad of [
      "https://sandbox.example",
      "https://sandbox.example/{label}",
      "https://{label}@sandbox.example",
      "https://user@{label}.sandbox.example",
      "ftp://{label}.sandbox.example",
      "https://{label}.sandbox.example/path",
      "https://{label}.sandbox.example/?q=1",
      "not a url {label}",
    ])
      expect(() => originFromTemplate(bad, "abc")).toThrow();
    expect(originFromTemplate("http://{label}.localhost:5180", "abc")).toBe(
      "http://abc.localhost:5180",
    );
  });
});

describe("sandboxUrl", () => {
  test("carries what the loader needs and nothing secret", () => {
    const url = sandboxUrl({
      origin: "http://abc.localhost:5180",
      cid: "bafyexample",
      gateway: "https://gateway.example",
      hostOrigin: "http://127.0.0.1:5180",
      kind: "car",
      start: "/a/b?x=1#top",
      container: true,
      owner: "abc-123",
    });
    expect(url.origin).toBe("http://abc.localhost:5180");
    expect(url.pathname).toBe("/__sandbox/index.html");
    expect(Object.fromEntries(url.searchParams)).toEqual({
      cid: "bafyexample",
      gateway: "https://gateway.example",
      host: "http://127.0.0.1:5180",
      kind: "car",
      start: "/a/b?x=1#top",
      owner: "abc-123",
    });
  });

  test("marks a container-less start", () => {
    const url = sandboxUrl({
      origin: "http://abc.localhost:5180",
      cid: "b",
      gateway: "https://g.example",
      hostOrigin: "http://127.0.0.1:5180",
      kind: "site",
      start: "/",
      container: false,
      owner: "abc-123",
    });
    expect(url.searchParams.get("container")).toBe("off");
  });
});

describe("requireSecureHost", () => {
  test("lets a secure page through", () => {
    expect(() => requireSecureHost(true, "https://host.example")).not.toThrow();
  });

  // The product's own origin can be trustworthy and still get no service worker,
  // because a frame under an insecure page is not a secure context.
  test("refuses an insecure page and names it, whatever origin the product would get", () => {
    expect(() => requireSecureHost(false, "http://192.168.0.7:5180")).toThrow(
      "http://192.168.0.7:5180",
    );
    expect(() => requireSecureHost(false, "http://192.168.0.7:5180")).toThrow(
      "secure page",
    );
  });
});

describe("a port range in place of a label", () => {
  const host = at("https://host.example/");
  const range = "https://host.example:{9450-9452}";

  // One name cannot tell products apart by host, so the port must, and two
  // products on one port would share whatever one of them stored there.
  test("gives each product its own port, and the same product the same port", async () => {
    const ports = ledger();
    const origin = (id: string) =>
      sandboxOrigin(host, id, { template: range, ports });
    const [a, b, c] = [
      await origin("a.paseo"),
      await origin("b.paseo"),
      await origin("c.paseo"),
    ];
    expect([a, b, c]).toEqual([
      "https://host.example:9450",
      "https://host.example:9451",
      "https://host.example:9452",
    ]);
    expect(await origin("b.paseo")).toBe(b);
    expect(await origin("a.paseo")).toBe(a);
  });

  test("never hands a port to a second product: a full range is an error", async () => {
    const ports = ledger();
    for (const id of ["a.paseo", "b.paseo", "c.paseo"])
      await sandboxOrigin(host, id, { template: range, ports });
    await expect(
      sandboxOrigin(host, "d.paseo", { template: range, ports }),
    ).rejects.toThrow("All 3 sandbox ports are owned or retired");
  });

  test("does not reuse a port after the range moves", async () => {
    const ports = ledger();
    await sandboxOrigin(host, "a.paseo", { template: range, ports });
    await expect(
      sandboxOrigin(host, "a.paseo", {
        template: "https://host.example:{9460-9462}",
        ports,
      }),
    ).rejects.toThrow("outside the range");
  });

  test("refuses a range that includes the host's own port, default or named", async () => {
    await expect(
      sandboxOrigin(host, "a.paseo", {
        template: "https://host.example:{400-450}",
        ports: ledger(),
      }),
    ).rejects.toThrow("host's own port");
    await expect(
      sandboxOrigin(at("https://host.example:9451/"), "a.paseo", {
        template: range,
        ports: ledger(),
      }),
    ).rejects.toThrow("host's own port");
  });

  test("refuses ranges that are backwards, out of bounds or too wide", async () => {
    for (const bad of ["{9452-9450}", "{0-10}", "{9000-70000}", "{1025-1200}"])
      await expect(
        sandboxOrigin(host, "a.paseo", {
          template: `https://host.example:${bad}`,
          ports: ledger(),
        }),
      ).rejects.toThrow();
  });

  test("keeps the origin rules: no credentials, path or fragment", async () => {
    for (const bad of [
      "https://user@host.example:{9450-9452}",
      "https://host.example:{9450-9452}/path",
      "ftp://host.example:{9450-9452}",
    ])
      await expect(
        sandboxOrigin(host, "a.paseo", { template: bad, ports: ledger() }),
      ).rejects.toThrow();
  });

  test("gives no port when the browser has no Web Locks", async () => {
    await expect(
      sandboxOrigin(host, "a.paseo", {
        template: range,
        ports: new PortLedger({ getItem: () => null, setItem: () => {} }, null),
      }),
    ).rejects.toThrow("no Web Locks");
  });
});

describe("containerOffAllowed", () => {
  const on = (url: string, template = "") =>
    containerOffAllowed(new URL(url), template);

  // Cookies are keyed by host name, so anything but a `.localhost` label of its
  // own lets one product's cookies reach its neighbours and the wallet host's name.
  test("only a loopback host with the automatic .localhost origins may drop the container", () => {
    expect(on("http://localhost:5180/")).toBe(true);
    expect(on("http://127.0.0.1:5180/")).toBe(true);
  });

  test("a port range on one name may not, nor a label template, nor a LAN or tailnet host", () => {
    expect(
      on("https://host.example/", "https://host.example:{9450-9459}"),
    ).toBe(false);
    expect(
      on("http://localhost:5180/", "https://host.example:{9450-9459}"),
    ).toBe(false);
    expect(on("http://localhost:5180/", "http://{label}.localhost:5180")).toBe(
      false,
    );
    expect(on("https://host.example/")).toBe(false);
    expect(on("http://192.168.0.7:5180/")).toBe(false);
  });
});
