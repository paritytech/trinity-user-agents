import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { TOKEN_MS, createFieldImages } from "../../field-image-plugin.js";
import { detailSvg } from "../../src/locket/svg.js";

/**
 * The dev server's field images over real HTTP on loopback, as raw clients
 * reach them: what each refusal needs, single use, expiry and revoke. What
 * the browser adds (the Referer it sets, the pixels it keeps from a
 * product) is in `browser/field-demo.spec.ts`.
 */

const PRODUCT = "http://localhost:5183";
const SVG = detailSvg(
  "address",
  ["Ada Example", "24 Sample Lane", "Example City 01234"],
  "paper",
);
let clock = 1_800_000_000_000;
const images = createFieldImages(() => clock);
let server: Server;
let base: string;

beforeAll(async () => {
  server = createServer((request, response) => {
    const path = new URL(request.url ?? "/", "http://x").pathname.replace(
      /^\/__field-images\//,
      "",
    );
    // As Vite's own CORS middleware does before the plugin runs.
    response.setHeader("access-control-allow-origin", "http://localhost:5183");
    void images.handle(request, response, path);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  base = `http://localhost:${(server.address() as AddressInfo).port}/__field-images/`;
});
afterAll(() => server.close());

const host = { "x-field-images": "1", "sec-fetch-site": "same-origin" };

async function issue(origin = PRODUCT): Promise<string> {
  const answer = await fetch(`${base}issue`, {
    method: "POST",
    headers: { ...host, "content-type": "application/json" },
    body: JSON.stringify({ origin, svg: SVG }),
  });
  expect(answer.status).toBe(200);
  return ((await answer.json()) as { token: string }).token;
}

/** An image request, by default with the headers a browser sends for the product's `<img>`. */
const load = (
  token: string,
  headers: Record<string, string> = {},
  method = "GET",
) =>
  fetch(`${base}${token}.svg`, {
    method,
    headers: {
      "sec-fetch-dest": "image",
      referer: `${PRODUCT}/market.html`,
      ...headers,
    },
  });

describe("the dev server's field images", () => {
  test("serve once, uncached and with no CORS grant, to anything that sends the product's headers", async () => {
    const token = await issue();
    const answer = await load(token);
    expect({
      status: answer.status,
      type: answer.headers.get("content-type"),
      cache: answer.headers.get("cache-control"),
      cors: [...answer.headers.keys()].filter((name) =>
        name.startsWith("access-control"),
      ),
      csp: answer.headers.get("content-security-policy"),
      // This client is not a browser: the URL alone, with two forged headers, is enough.
      body: (await answer.text()).includes("24 Sample Lane"),
      again: (await load(token)).status,
    }).toEqual({
      status: 200,
      type: "image/svg+xml",
      cache: "no-store",
      cors: [],
      csp: "default-src 'none'; style-src 'unsafe-inline'; sandbox",
      body: true,
      again: 404,
    });
  });

  test("refuse another origin, destination, method or host name, and keep the token for the real request", async () => {
    const token = await issue();
    expect({
      otherOrigin: (await load(token, { referer: "http://localhost:5182/" }))
        .status,
      noReferer: (await load(token, { referer: "" })).status,
      asDocument: (await load(token, { "sec-fetch-dest": "iframe" })).status,
      asFetch: (await load(token, { "sec-fetch-dest": "empty" })).status,
      post: (await load(token, {}, "POST")).status,
      otherHost: (await load(token, { host: "tunnel.example" })).status,
      stillThere: (await load(token)).status,
    }).toEqual({
      otherOrigin: 404,
      noReferer: 404,
      asDocument: 404,
      asFetch: 404,
      post: 404,
      otherHost: 404,
      stillThere: 200,
    });
  });

  test("issue only to a same-origin call, and end tokens on expiry and revoke", async () => {
    const forged = await fetch(`${base}issue`, {
      method: "POST",
      headers: { "x-field-images": "1", "sec-fetch-site": "same-site" },
      body: JSON.stringify({ origin: PRODUCT, svg: SVG }),
    });
    const noHeader = await fetch(`${base}issue`, {
      method: "POST",
      headers: { "sec-fetch-site": "same-origin" },
      body: JSON.stringify({ origin: PRODUCT, svg: SVG }),
    });
    const expiring = await issue();
    clock += TOKEN_MS + 1;
    const expired = (await load(expiring)).status;
    const revoked = await issue();
    const kept = await issue();
    await fetch(`${base}revoke`, {
      method: "POST",
      headers: host,
      body: JSON.stringify({ tokens: [revoked] }),
    });
    expect({
      forged: forged.status,
      noHeader: noHeader.status,
      expired,
      revoked: (await load(revoked)).status,
      kept: (await load(kept)).status,
    }).toEqual({
      forged: 404,
      noHeader: 404,
      expired: 404,
      revoked: 404,
      kept: 200,
    });
  });

  test("store only an SVG document of bounded size", async () => {
    const issue = (svg: string) =>
      fetch(`${base}issue`, {
        method: "POST",
        headers: host,
        body: JSON.stringify({ origin: PRODUCT, svg }),
      });
    expect({
      html: (await issue("<html><script>alert(1)</script></html>")).status,
      large: (await issue(SVG + " ".repeat(8192))).status,
      fine: (await issue(SVG)).status,
    }).toEqual({ html: 404, large: 404, fine: 200 });
  });
});
