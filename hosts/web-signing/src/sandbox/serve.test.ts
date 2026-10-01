import { describe, expect, test } from "bun:test";
import {
  PRODUCT_CSP,
  archivePath,
  archiveRequestPath,
  contentTypeOf,
  injectContainer,
  responseFor,
} from "./serve.js";

const HOST = "http://127.0.0.1:5180";
const CONTAINER = "/repo/truapi-sandbox/container.js";

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
  const scripts = (html: string) => html.indexOf(CONTAINER);

  test("goes before every script of the page, whatever the head holds", () => {
    const html =
      "<!doctype html><html><head><script>window.mine=1</script></head></html>";
    const injected = injectContainer(html, HOST, CONTAINER);
    expect(scripts(injected)).toBeGreaterThan(-1);
    expect(scripts(injected)).toBeLessThan(injected.indexOf("window.mine"));
    expect(injected.startsWith("<!doctype html><script>")).toBe(true);
  });

  test("keeps a comment before the doctype, so the page stays out of quirks mode", () => {
    const injected = injectContainer(
      "<!-- built --><!DOCTYPE html><p>x</p>",
      HOST,
      CONTAINER,
    );
    expect(injected.startsWith("<!-- built --><!DOCTYPE html><script>")).toBe(
      true,
    );
  });

  test("goes first when the page has no doctype", () => {
    expect(
      injectContainer("<p>x</p>", HOST, CONTAINER).startsWith("<script>"),
    ).toBe(true);
  });

  test("pins the port to the host's origin, and a hostile one cannot break out of the script", () => {
    expect(injectContainer("<!doctype html>", HOST, CONTAINER)).toContain(
      `window.__truapi_message_port="${HOST}"`,
    );
    const hostile = injectContainer(
      "<!doctype html>",
      "</script><script>alert(1)",
      CONTAINER,
    );
    expect(hostile).not.toContain("</script><script>alert(1)");
  });
});

describe("responseFor", () => {
  const html = new TextEncoder().encode("<!doctype html><title>t</title>");
  const WITH_CONTAINER = { hostOrigin: HOST, src: CONTAINER };

  test("serves HTML with the container and a policy against embedding other sites", async () => {
    const response = responseFor(
      { path: "index.html", bytes: html },
      WITH_CONTAINER,
    );
    expect(response.headers.get("content-security-policy")).toBe(PRODUCT_CSP);
    expect(response.headers.get("content-type")).toBe(
      "text/html; charset=utf-8",
    );
    expect(await response.text()).toContain(CONTAINER);
  });

  test("serves other files untouched and never sniffed", async () => {
    const bytes = new Uint8Array([1, 2, 3]);
    const response = responseFor({ path: "data.wasm", bytes }, WITH_CONTAINER);
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

describe("archiveRequestPath", () => {
  const scope = "/repo/product/0123456789ab/app-1/bafyc/";

  test("strips the mount path from a product's own relative links", () => {
    expect(archiveRequestPath(`${scope}assets/app.js`, scope)).toBe(
      "assets/app.js",
    );
    expect(archiveRequestPath(scope, scope)).toBe("index.html");
  });

  // A build made for the site root links `/assets/app.js`. Under a project path
  // that is outside the mount, and it must still find the archive's file.
  test("takes a root-relative link as the archive path itself", () => {
    expect(archiveRequestPath("/assets/app.js", scope)).toBe("assets/app.js");
    expect(archiveRequestPath("/", scope)).toBe("index.html");
  });

  test("never resolves a path that climbs out of the archive", () => {
    expect(archiveRequestPath(`${scope}../../secret`, scope)).toBeNull();
    expect(archiveRequestPath("/a/../b", scope)).toBeNull();
  });
});
