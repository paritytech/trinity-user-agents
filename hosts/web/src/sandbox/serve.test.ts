import { describe, expect, test } from "bun:test";
import {
  archivePath,
  archiveRequestPath,
  contentTypeOf,
  responseFor,
} from "./serve.js";

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

describe("responseFor", () => {
  test("serves HTML as it is, with its type and no sniffing", async () => {
    const html = "<!doctype html><title>t</title>";
    const response = responseFor({
      path: "index.html",
      bytes: new TextEncoder().encode(html),
    });
    expect(response.headers.get("content-type")).toBe(
      "text/html; charset=utf-8",
    );
    expect(response.headers.get("content-security-policy")).toBeNull();
    expect(await response.text()).toBe(html);
  });

  test("serves other files untouched and never sniffed", async () => {
    const bytes = new Uint8Array([1, 2, 3]);
    const response = responseFor({ path: "data.wasm", bytes });
    expect(response.headers.get("content-type")).toBe("application/wasm");
    expect(response.headers.get("x-content-type-options")).toBe("nosniff");
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(bytes);
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
