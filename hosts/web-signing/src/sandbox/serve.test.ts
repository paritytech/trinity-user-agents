import { describe, expect, test } from "bun:test";
import {
  PRODUCT_CSP,
  archivePath,
  containerWanted,
  contentTypeOf,
  injectContainer,
  responseFor,
} from "./serve.js";

const HOST = "http://127.0.0.1:5180";

describe("archivePath", () => {
  test("maps the root and directories to their index.html", () => {
    expect(archivePath("/")).toBe("index.html");
    expect(archivePath("/docs/")).toBe("docs/index.html");
    expect(archivePath("/assets/app.js")).toBe("assets/app.js");
    expect(archivePath("/a%20b.txt")).toBe("a b.txt");
  });

  test("never resolves a path that climbs, doubles a slash or hides a separator", () => {
    for (const bad of [
      "/../secret",
      "/a/../b",
      "/./a",
      "/a//b",
      "/%2e%2e/secret",
      "/a%2f..%2fb",
      "/a%5cb",
      "/a%00b",
      "/%zz",
    ])
      expect(archivePath(bad)).toBeNull();
  });
});

describe("injectContainer", () => {
  const scripts = (html: string) => html.indexOf("/__sandbox/container.js");

  test("goes before every script of the page, whatever the head holds", () => {
    const html =
      "<!doctype html><html><head><script>window.mine=1</script></head></html>";
    const injected = injectContainer(html, HOST);
    expect(scripts(injected)).toBeGreaterThan(-1);
    expect(scripts(injected)).toBeLessThan(injected.indexOf("window.mine"));
    expect(injected.startsWith("<!doctype html><script>")).toBe(true);
  });

  test("keeps a comment before the doctype, so the page stays out of quirks mode", () => {
    const injected = injectContainer(
      "<!-- built --><!DOCTYPE html><p>x</p>",
      HOST,
    );
    expect(injected.startsWith("<!-- built --><!DOCTYPE html><script>")).toBe(
      true,
    );
  });

  test("goes first when the page has no doctype", () => {
    expect(injectContainer("<p>x</p>", HOST).startsWith("<script>")).toBe(true);
  });

  test("pins the port to the host's origin, and a hostile one cannot break out of the script", () => {
    expect(injectContainer("<!doctype html>", HOST)).toContain(
      `window.__truapi_message_port="${HOST}"`,
    );
    const hostile = injectContainer(
      "<!doctype html>",
      "</script><script>alert(1)",
    );
    expect(hostile).not.toContain("</script><script>alert(1)");
  });
});

describe("responseFor", () => {
  const html = new TextEncoder().encode("<!doctype html><title>t</title>");

  test("serves HTML with the container and a policy against embedding other sites", async () => {
    const response = responseFor({ path: "index.html", bytes: html }, HOST);
    expect(response.headers.get("content-security-policy")).toBe(PRODUCT_CSP);
    expect(response.headers.get("content-type")).toBe(
      "text/html; charset=utf-8",
    );
    expect(await response.text()).toContain("/__sandbox/container.js");
  });

  test("serves other files untouched and never sniffed", async () => {
    const bytes = new Uint8Array([1, 2, 3]);
    const response = responseFor({ path: "data.wasm", bytes }, HOST);
    expect(response.headers.get("content-type")).toBe("application/wasm");
    expect(response.headers.get("x-content-type-options")).toBe("nosniff");
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(bytes);
  });

  test("serves HTML as it is only when the developer took the container out", async () => {
    const response = responseFor({ path: "index.html", bytes: html }, null);
    expect(response.headers.get("content-security-policy")).toBeNull();
    expect(await response.text()).toBe("<!doctype html><title>t</title>");
  });

  test("does not guess a type for an unknown extension", () => {
    expect(contentTypeOf("blob.xyz")).toBe("application/octet-stream");
  });
});

describe("containerWanted", () => {
  // A link to a port origin can carry `container=off`. Only a `.localhost`
  // origin may honour it, so a crafted link cannot switch the container off
  // where products share cookies.
  test("honours a request to drop the container only on a .localhost origin", () => {
    expect(containerWanted(false, "abc-123.localhost")).toBe(false);
    expect(containerWanted(false, "host.example")).toBe(true);
    expect(containerWanted(false, "192.168.0.7")).toBe(true);
    expect(containerWanted(false, "evil.localhost.example")).toBe(true);
  });

  test("keeps it on when it was not asked to drop it", () => {
    expect(containerWanted(true, "abc-123.localhost")).toBe(true);
    expect(containerWanted(true, "host.example")).toBe(true);
  });
});
