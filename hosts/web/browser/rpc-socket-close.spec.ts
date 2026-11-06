/**
 * The provider's RPC WebSocket after its connection is released or its
 * handshake fails. The host
 * opens one People connection per Statement Store subscription and closes it
 * when the subscription ends; the browser still dispatches the socket's
 * `close` event after that, and when the connection was freed first (the
 * garbage collector does it any time after `close()`), a handler left on the
 * socket would call a dropped closure and throw "closure invoked recursively
 * or after being dropped" into the page.
 *
 * A handshake that fails drops the same handlers, and the socket's `close`
 * event follows its `error`.
 *
 * Loopback only: a WebSocket server in this process answers the one request.
 * It needs a Chromium and `@parity/truapi-provider`'s built bundle.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { fileURLToPath } from "node:url";
import type { Browser } from "playwright-core";
import { testBrowser } from "./support/fixtures.js";

const GENESIS = `0x${"11".repeat(32)}`;
const dist = fileURLToPath(
  new URL("../node_modules/@parity/truapi-provider/dist/", import.meta.url),
);

let browser: Browser;
let server: ReturnType<typeof Bun.serve>;

beforeAll(async () => {
  browser = await testBrowser();
  server = Bun.serve({
    port: 0,
    hostname: "127.0.0.1",
    fetch(request, self) {
      const path = new URL(request.url).pathname;
      if (path === "/rpc")
        return self.upgrade(request, { data: undefined })
          ? undefined
          : new Response("no", { status: 400 });
      if (path === "/")
        return new Response(page(self.port!), {
          headers: { "content-type": "text/html" },
        });
      return new Response(Bun.file(`${dist}${path.slice(1)}`), {
        headers: {
          "content-type": path.endsWith(".wasm")
            ? "application/wasm"
            : "text/javascript",
        },
      });
    },
    websocket: {
      message(socket, message) {
        const { id } = JSON.parse(String(message)) as { id: number };
        socket.send(JSON.stringify({ jsonrpc: "2.0", id, result: "loopback" }));
      },
    },
  });
});

afterAll(() => {
  server?.stop();
});

const page = (port: number) => `<!doctype html><script type="module">
import init, { ChainProviderBuilder } from "/truapi_provider.js";
window.closeAndRelease = async () => {
  await init({ module_or_path: "/truapi_provider_bg.wasm" });
  const builder = new ChainProviderBuilder();
  builder.addRpcChain("${GENESIS}", "ws://127.0.0.1:${port}/rpc");
  const provider = builder.build();
  const connection = await provider.connect("${GENESIS}");
  connection.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "system_chain", params: [] }));
  const answer = await connection.nextResponse();
  connection.close();
  connection.free();
  // The server's close frame, and the socket's close event, come after this.
  await new Promise((resolve) => setTimeout(resolve, 1000));
  return answer;
};
window.failHandshake = async () => {
  await init({ module_or_path: "/truapi_provider_bg.wasm" });
  const builder = new ChainProviderBuilder();
  builder.addRpcChain("${GENESIS}", "ws://127.0.0.1:1/rpc");
  const failure = await builder.build().connect("${GENESIS}").then(
    () => "connected",
    (error) => String(error.message ?? error),
  );
  // The socket's close event comes after its error, which failed the connect.
  await new Promise((resolve) => setTimeout(resolve, 1000));
  return failure;
};
</script>`;

async function run(script: string): Promise<{
  answer: string;
  errors: string[];
}> {
  const tab = await browser.newPage();
  const errors: string[] = [];
  tab.on("pageerror", (error) => errors.push(error.message));
  try {
    await tab.goto(`http://127.0.0.1:${server.port}/`);
    await tab.waitForFunction((name) => name in window, script);
    const answer = await tab.evaluate(
      (name) =>
        (window as unknown as Record<string, () => Promise<string>>)[name]!(),
      script,
    );
    return { answer, errors };
  } finally {
    await tab.close();
  }
}

describe("the provider's RPC socket", () => {
  test("a connection released before its socket's close event throws nothing into the page", async () => {
    const { answer, errors } = await run("closeAndRelease");
    expect([JSON.parse(answer).result, errors]).toEqual(["loopback", []]);
  });

  test("a connect whose handshake fails throws nothing into the page", async () => {
    const { answer, errors } = await run("failHandshake");
    expect([answer.includes("WebSocket"), errors]).toEqual([true, []]);
  });
});
