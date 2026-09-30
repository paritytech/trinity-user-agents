import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { request, type Server } from "node:http";
import { createServer as listenOn } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { build, createServer, preview, type PreviewServer } from "vite";
import { sandboxAssets } from "../../sandbox-plugin.js";
import { productLabel } from "./origin.js";
import type { SandboxPolicy } from "./policy.js";

const policy: SandboxPolicy = { template: "", hostOrigins: [] };
const product = `${productLabel("a.paseo")}.localhost`;

async function freePort(): Promise<number> {
  return new Promise((resolve) => {
    const probe = listenOn().listen(0, "127.0.0.1", () => {
      const { port } = probe.address() as { port: number };
      probe.close(() => resolve(port));
    });
  });
}

/** What a server answers a browser at `host` asking for `path`. `fetch` cannot set `Host`. */
function answerTo(
  port: number,
  host: string,
  path: string,
  headers: Record<string, string> = {},
): Promise<{ status: number; body: string }> {
  return new Promise((resolve, reject) => {
    const asked = request(
      {
        host: "127.0.0.1",
        port,
        path,
        headers: { host: `${host}:${port}`, ...headers },
      },
      (answer) => {
        const chunks: Buffer[] = [];
        answer.on("data", (chunk: Buffer) => chunks.push(chunk));
        answer.on("end", () =>
          resolve({
            status: answer.statusCode ?? 0,
            body: Buffer.concat(chunks).toString("utf8"),
          }),
        );
      },
    );
    asked.on("error", reject);
    asked.end();
  });
}

const IFRAME = { "sec-fetch-dest": "iframe" };

/**
 * The same paths reached by spelling, which a static file server may decode
 * before it finds the file. On the wallet's own origin every one must be
 * refused, not just the plain spellings.
 */
const SPELLINGS = [
  "/__sandbox/index.html",
  "/__sandbox/%69ndex.html",
  "/__sandbox/page.js",
  "/__sandbox/%70age.js",
  "/__sandbox/container.js",
  "/__sandbox/%63ontainer.js",
  "/%5f%5fsandbox/page.js",
  "/__SANDBOX/page.js",
  "/__sandbox/",
  "/__sandbox",
  "//__sandbox/page.js",
  "/x/../__sandbox/%70age.js",
  "/__sandbox/%2e/page.js",
  "/__sandbox/%2570age.js",
  "/__sandbox\\page.js",
];

let root: string;
let port: number;
const closers: (() => Promise<unknown>)[] = [];

beforeAll(async () => {
  root = mkdtempSync(join(tmpdir(), "sandbox-plugin-"));
  writeFileSync(
    join(root, "index.html"),
    "<!doctype html><title>wallet</title>",
  );
  await build({
    root,
    configFile: false,
    logLevel: "silent",
    plugins: [sandboxAssets(policy)],
  });
}, 60_000);

afterAll(async () => {
  for (const close of closers) await close();
});

/** Each server type is what the host runs: `vite` in development, `vite preview` on a build. */
const servers: [string, () => Promise<number>][] = [
  [
    "preview of a build",
    async () => {
      const server: PreviewServer = await preview({
        root,
        configFile: false,
        logLevel: "silent",
        plugins: [sandboxAssets(policy)],
        preview: {
          port: await freePort(),
          host: "127.0.0.1",
          strictPort: true,
        },
      });
      closers.push(
        () => new Promise((done) => (server.httpServer as Server).close(done)),
      );
      return (server.httpServer.address() as { port: number }).port;
    },
  ],
  [
    "dev server",
    async () => {
      const server = await createServer({
        root,
        configFile: false,
        logLevel: "silent",
        plugins: [sandboxAssets(policy)],
        server: {
          port: await freePort(),
          host: "127.0.0.1",
          strictPort: true,
          hmr: false,
        },
      });
      await server.listen();
      closers.push(() => server.close());
      return (server.httpServer?.address() as { port: number }).port;
    },
  ],
];

for (const [name, start] of servers)
  describe(name, () => {
    let sandboxFiles: string[];

    beforeAll(async () => {
      port = await start();
      sandboxFiles = (
        await Promise.all(
          [
            "/__sandbox/index.html",
            "/__sandbox/page.js",
            "/__sandbox/container.js",
          ].map((path) => answerTo(port, product, path, IFRAME)),
        )
      ).map((answer) => {
        expect(answer.status).toBe(200);
        return answer.body;
      });
    }, 30_000);

    // A refusal is an HTTP answer. A dropped connection would pass a "did not
    // get the file" check for the wrong reason. A server may answer a path it
    // has no file for with the wallet's own page; that is fine, the sandbox's
    // files are not.
    test("gives the wallet's origin no sandbox file under any spelling", async () => {
      for (const host of ["localhost", "127.0.0.1"])
        for (const path of SPELLINGS) {
          const { status, body } = await answerTo(port, host, path, IFRAME);
          expect({
            host,
            path,
            status,
            sandboxFile: sandboxFiles.includes(body),
          }).toEqual({
            host,
            path,
            status,
            sandboxFile: false,
          });
        }
    });

    test("refuses the plain spellings with a 404", async () => {
      for (const path of [
        "/__sandbox/index.html",
        "/__sandbox/page.js",
        "/__sandbox/container.js",
      ])
        expect((await answerTo(port, "localhost", path, IFRAME)).status).toBe(
          404,
        );
    });
  });
