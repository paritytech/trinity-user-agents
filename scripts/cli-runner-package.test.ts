import { afterAll, beforeAll, expect, it } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createContext, runInContext } from "node:vm";
import {
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_RESPONSE,
  type ProtocolMessage,
  scale,
  VersionedRemotePermissionRequest,
  VersionedRemotePermissionResponse,
  VersionedRemotePermissionError,
  VersionedHostHandshakeResponse,
  VersionedHostHandshakeError,
} from "@parity/truapi";
import { PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION, SYSTEM_HANDSHAKE } from "../js/packages/truapi/src/generated/wire-table.ts";

function replyToHealth(socket: { send(bytes: Uint8Array): unknown }, request: ProtocolMessage): boolean {
  if (request.payload.traitId !== SYSTEM_HANDSHAKE.trait ||
      request.payload.methodId !== SYSTEM_HANDSHAKE.method) return false;
  socket.send(encodeWireMessage({
    ...request,
    payload: {
      ...request.payload,
      messageType: MESSAGE_TYPE_RESPONSE,
      value: scale.Result(VersionedHostHandshakeResponse, scale.CallError(VersionedHostHandshakeError))
        .enc({ success: true, value: { tag: "V1", value: undefined } }),
    },
  })._unsafeUnwrap());
  return true;
}

const repository = resolve(import.meta.dir, "..");
let directory: string;

beforeAll(async () => {
  directory = await mkdtemp(join(tmpdir(), "truapi-cli-package-"));
  const result = Bun.spawnSync(
    ["make", "cli-runner", `CLI_DIST_DIR=${directory}`],
    {
      cwd: repository,
      env: {
        ...process.env,
        PATH: `${dirname(process.execPath)}:${process.env.PATH}`,
      },
    },
  );
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
}, 60_000);

afterAll(async () => {
  if (directory) await rm(directory, { recursive: true, force: true });
});

it("resolves the packaged runner without a source checkout", async () => {
  const preload = join(directory, "stack-format.js");
  await writeFile(
    preload,
    'Error.prepareStackTrace = () => "Error\\n    at package-test";',
  );
  for (const args of [[], ["--preload", preload]]) {
    const result = Bun.spawnSync(
      [process.execPath, ...args, join(directory, "runner.js")],
      {
        cwd: tmpdir(),
        env: { PATH: process.env.PATH },
      },
    );
    expect({
      status: result.exitCode,
      output: result.stdout.toString() + result.stderr.toString(),
    }).toEqual({
      status: 1,
      output: expect.stringContaining("TRUAPI_FRAME_URL must be set"),
    });
  }
});

it("shares web permissions with installed scripts and the injected browser client", async () => {
  const authorizations: unknown[] = [];
  const decisions: boolean[] = [];
  const requests: string[] = [];
  let granted = false;
  let connections = 0;
  let respond = true;
  let closeHost = () => {};
  const server = Bun.serve<{ frames: boolean }>({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request, server) {
      if (
        server.upgrade(request, {
          data: { frames: new URL(request.url).pathname === "/frames" },
        })
      )
        return;
      requests.push(new URL(request.url).pathname);
      return new Response("http-ok");
    },
    websocket: {
      open(socket) {
        if (socket.data.frames) {
          connections++;
          closeHost = () => socket.close();
        }
      },
      message(socket, message) {
        if (!socket.data.frames) {
          socket.send(message);
          return;
        }
        const request = decodeWireMessage(
          new Uint8Array(message as Buffer),
        )._unsafeUnwrap();
        if (replyToHealth(socket, request)) return;
        if (!respond) return;
        let allowed = true;
        if (
          request.payload.methodId ===
          PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION.method
        ) {
          allowed = granted;
          granted = false;
          decisions.push(allowed);
          authorizations.push(
            VersionedRemotePermissionRequest.dec(request.payload.value),
          );
        } else granted = true;
        socket.send(
          encodeWireMessage({
            ...request,
            payload: {
              ...request.payload,
              messageType: MESSAGE_TYPE_RESPONSE,
              value: scale
                .Result(
                  VersionedRemotePermissionResponse,
                  scale.CallError(VersionedRemotePermissionError),
                )
                .enc({
                  success: true,
                  value: { tag: "V1", value: { granted: allowed } },
                }),
            },
          })._unsafeUnwrap(),
        );
      },
    },
  });
  const script = join(directory, "websocket-product.ts");
  await writeFile(
    script,
    `
    import { readFileSync, writeFileSync } from 'node:fs';
    import { join } from 'node:path';
    assert(window === window.top && window.__HOST_WEBVIEW_MARK__);
    assert(window.__HOST_API_CLIENT__.client === truapi);
    assert(window.__HOST_API_PORT__ instanceof MessagePort);
    assert(Object.getOwnPropertyDescriptor(globalThis, 'fetch').configurable === false);
    const expected: string = process.env.PACKAGED_TEST_VALUE!;
    const report = join(import.meta.dir, 'report.txt');
    writeFileSync(report, expected);
    assert(readFileSync(report, 'utf8') === expected);
    async function allowOnce() {
      assert((await truapi.permissions.requestRemotePermission({
        permission: { tag: 'Remote', value: { domains: ['127.0.0.1'] } },
      }))._unsafeUnwrap().granted);
    }
    await allowOnce();
    assert(await (await fetch('http://127.0.0.1:${server.port}/allowed')).text() === 'http-ok');
    try { await fetch('http://127.0.0.1:${server.port}/denied'); throw new Error('grant reused'); }
    catch (error) { assert(error instanceof TypeError); }
    await allowOnce();
    await new Promise<void>((resolve, reject) => {
      const socket = new WebSocket('ws://127.0.0.1:${server.port}/echo');
      socket.onopen = () => socket.send(expected);
      socket.onmessage = ({ data }) => {
        if (data !== expected) return reject(new Error('Unexpected reply'));
        socket.close();
        console.log(data);
        resolve();
      };
      socket.onerror = () => reject(new Error('WebSocket failed'));
    });
  `,
  );
  const child = Bun.spawn([process.execPath, join(directory, "runner.js")], {
    cwd: tmpdir(),
    env: {
      PATH: process.env.PATH,
      PACKAGED_TEST_VALUE: "packaged-websocket-ok",
      TRUAPI_FRAME_URL: `ws://127.0.0.1:${server.port}/frames`,
      TRUAPI_PRODUCT_ID: "package-test.dot",
      TRUAPI_SCRIPT: script,
    },
    stdout: "pipe",
    stderr: "pipe",
  });
  const timeout = setTimeout(() => child.kill(), 15_000);
  try {
    const [status, output, error] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(
      {
        status,
        output,
        report: await readFile(join(directory, "report.txt"), "utf8").catch(
          () => null,
        ),
        requests,
        authorizations,
        decisions,
        connections,
      },
      error,
    ).toEqual({
      status: 0,
      output: "packaged-websocket-ok\n",
      report: "packaged-websocket-ok",
      requests: ["/allowed"],
      decisions: [true, false, true],
      connections: 1,
      authorizations: Array(3).fill({
        tag: "V1",
        value: {
          permission: { tag: "Remote", value: { domains: ["127.0.0.1"] } },
        },
      }),
    });
    class BrowserSocket extends WebSocket {}
    for (const [name, descriptor] of Object.entries(
      Object.getOwnPropertyDescriptors(WebSocket.prototype),
    )) {
      if (name !== "constructor")
        Object.defineProperty(BrowserSocket.prototype, name, descriptor);
    }
    const events = new EventTarget();
    const context = createContext({
      MessageChannel,
      MessagePort,
      MessageEvent,
      Event,
      EventTarget,
      CloseEvent,
      WebSocket: BrowserSocket,
      TextEncoder,
      TextDecoder,
      URL,
      Uint8Array,
      ArrayBuffer,
      Request,
      Response,
      AbortController,
      AbortSignal,
      DOMException,
      Blob,
      fetch,
      performance,
      setTimeout,
      clearTimeout,
      navigator: {},
      document: Object.assign(new EventTarget(), { createElement: () => ({}), visibilityState: "hidden" }),
      location: { href: "https://product.example/" },
      addEventListener: events.addEventListener.bind(events),
      removeEventListener: events.removeEventListener.bind(events),
      dispatchEvent: events.dispatchEvent.bind(events),
      __truapi_localhost: { url: `ws://127.0.0.1:${server.port}/frames` },
    });
    runInContext("window = globalThis", context);
    runInContext(
      await readFile(join(directory, "sandbox-assets/container.js"), "utf8"),
      context,
    );
    const client = context.__HOST_API_CLIENT__.client;
    const permission = {
      permission: { tag: "Remote" as const, value: { domains: ["127.0.0.1"] } },
    };
    try {
      expect(
        (
          await client.permissions.requestRemotePermission(permission)
        )._unsafeUnwrap(),
      ).toEqual({ granted: true });
      const url = `http://127.0.0.1:${server.port}/browser`;
      expect(await (await context.fetch(url)).text()).toBe("http-ok");
      await expect(context.fetch(url)).rejects.toThrow(
        "Network access is not allowed",
      );
      respond = false;
      const sdk = Promise.resolve(
        client.permissions.requestRemotePermission(permission),
      ).then(
        (result) => result.isErr(),
        () => true,
      );
      const denied = context.fetch(url).catch((error: Error) => error.message);
      closeHost();
      expect({
        sdkFailed: await sdk,
        denied: await denied,
        requests,
        decisions,
        connections,
      }).toEqual({
        sdkFailed: true,
        denied: "Network access is not allowed",
        requests: ["/allowed", "/browser"],
        decisions: [true, false, true, true, false],
        connections: 2,
      });
    } finally {
      events.dispatchEvent(new Event("pagehide"));
    }
  } finally {
    clearTimeout(timeout);
    child.kill();
    server.stop(true);
  }
}, 20_000);

it("keeps browser SDK calls and one-use permissions on the same connection", async () => {
  const sdk = await Bun.build({
    entrypoints: [join(repository, "js/packages/truapi/src/sandbox.ts")],
    target: "browser",
    format: "esm",
  });
  if (!sdk.success) throw new Error(sdk.logs.join("\n"));
  await Bun.write(join(directory, "sdk.js"), sdk.outputs[0]!);
  let connections = 0;
  let frames = 0;
  const requests: string[] = [];
  const decisions: boolean[] = [];
  const server = Bun.serve<{ granted: boolean }>({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request, server) {
      if (server.upgrade(request, { data: { granted: false } })) return;
      requests.push(new URL(request.url).pathname);
      return new Response("http-ok");
    },
    websocket: {
      open() {
        connections++;
      },
      message(socket, message) {
        const request = decodeWireMessage(
          new Uint8Array(message as Buffer),
        )._unsafeUnwrap();
        if (replyToHealth(socket, request)) return;
        frames++;
        if (frames > 3 && frames <= 5) {
          if (frames === 5) socket.close();
          return;
        }
        const consume =
          request.payload.methodId ===
          PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION.method;
        const granted = consume ? socket.data.granted : true;
        socket.data.granted = !consume;
        if (consume) decisions.push(granted);
        socket.send(
          encodeWireMessage({
            ...request,
            payload: {
              ...request.payload,
              messageType: MESSAGE_TYPE_RESPONSE,
              value: scale
                .Result(
                  VersionedRemotePermissionResponse,
                  scale.CallError(VersionedRemotePermissionError),
                )
                .enc({
                  success: true,
                  value: { tag: "V1", value: { granted } },
                }),
            },
          })._unsafeUnwrap(),
        );
      },
    },
  });
  const script = join(directory, "browser-product.ts");
  await writeFile(
    script,
    `
    import { getClientSync, subscribeConnectionStatus } from './sdk.js';
    import { strict as assert } from 'node:assert';
    const events = new EventTarget();
    Object.assign(globalThis, {
      window: globalThis,
      top: globalThis,
      document: Object.assign(new EventTarget(), { createElement: () => ({}), visibilityState: "hidden" }),
      navigator: {},
      location: { href: 'https://product.example/' },
      addEventListener: events.addEventListener.bind(events),
      removeEventListener: events.removeEventListener.bind(events),
      dispatchEvent: events.dispatchEvent.bind(events),
      __truapi_localhost: { url: 'ws://127.0.0.1:${server.port}/frames' },
    });
    await import('./sandbox-assets/container.js');
    const client = getClientSync();
    assert(client);
    let status;
    subscribeConnectionStatus((value) => status = value);
    const permission = { permission: { tag: 'Remote', value: { domains: ['127.0.0.1'] } } };
    assert((await client.permissions.requestRemotePermission(permission))._unsafeUnwrap().granted);
    assert.equal(await (await fetch('http://127.0.0.1:${server.port}/allowed')).text(), 'http-ok');
    await assert.rejects(fetch('http://127.0.0.1:${server.port}/denied'), /Network access is not allowed/);
    const pendingSdk = Promise.resolve(client.permissions.requestRemotePermission(permission)).then(
      (result) => result.isErr(), () => true,
    );
    const pendingFetch = fetch('http://127.0.0.1:${server.port}/pending').then(() => false, () => true);
    const sdkFailed = await pendingSdk;
    const fetchFailed = await pendingFetch;
    assert.equal(getClientSync(), client);
    assert((await client.permissions.requestRemotePermission(permission))._unsafeUnwrap().granted);
    assert.equal(await (await fetch('http://127.0.0.1:${server.port}/recovered')).text(), 'http-ok');
    await assert.rejects(fetch('http://127.0.0.1:${server.port}/denied-again'), /Network access is not allowed/);
    console.log(JSON.stringify({ sdkFailed, fetchFailed, status }));
    events.dispatchEvent(new Event('pagehide'));
  `,
  );
  const child = Bun.spawn([process.execPath, script], {
    cwd: directory,
    stdout: "pipe",
    stderr: "pipe",
  });
  const timeout = setTimeout(() => child.kill(), 10_000);
  try {
    const [status, output, error] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    expect(
      {
        status,
        output: output.trim(),
        connections,
        frames,
        requests,
        decisions,
      },
      error,
    ).toEqual({
      status: 0,
      output: JSON.stringify({
        sdkFailed: true,
        fetchFailed: true,
        status: "connected",
      }),
      connections: 2,
      frames: 8,
      requests: ["/allowed", "/recovered"],
      decisions: [true, false, true, false],
    });
  } finally {
    clearTimeout(timeout);
    child.kill();
    server.stop(true);
  }
}, 15_000);
