import { afterAll, describe, expect, test as serialTest } from "bun:test";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Every test drives tailscale-serve.sh against a fake `tailscale` first on PATH.
// The fake keeps the Serve config in a temp file, so no real node or route is touched.

const SCRIPT = join(import.meta.dir, "tailscale-serve.sh");
const FAKE = join(import.meta.dir, "testing", "fake-tailscale.py");
const NODE = "node-a.tailnet.example";
const BACKEND = "http://127.0.0.1:5181";
const OTHER_BACKEND = "http://127.0.0.1:9999";
const PORTS = [443, 9450, 9451];

type Serve = { TCP?: Record<string, unknown>; Web?: Record<string, unknown>; AllowFunnel?: Record<string, boolean> };

// Each test waits on several short-lived processes, so they run side by side.
const test = serialTest.concurrent;

const roots: string[] = [];
afterAll(() => roots.forEach((root) => rmSync(root, { recursive: true, force: true })));

/** A Serve config holding one proxy route per port. */
function routes(ports: number[], backend = BACKEND): Serve {
  const serve: Serve = { TCP: {}, Web: {} };
  for (const port of ports) {
    serve.TCP![port] = { HTTPS: true };
    serve.Web![`${NODE}:${port}`] = { Handlers: { "/": { Proxy: backend } } };
  }
  return serve;
}

function setup(serve: Serve = {}) {
  const root = mkdtempSync(join(tmpdir(), "tailscale-serve-test-"));
  roots.push(root);
  mkdirSync(join(root, "bin"));
  symlinkSync(FAKE, join(root, "bin", "tailscale"));
  chmodSync(FAKE, 0o755);
  const configPath = join(root, "config.json");
  const logPath = join(root, "calls.log");
  const state = join(root, "state", "tailscale-serve.json");
  writeFileSync(configPath, JSON.stringify({ node: NODE, serve }));
  writeFileSync(logPath, "");
  const env: Record<string, string> = {
    PATH: `${join(root, "bin")}:${process.env.PATH}`,
    HOME: root,
    STATE: state,
    PORTS: "9450-9451",
    FAKE_TS_CONFIG: configPath,
    FAKE_TS_LOG: logPath,
  };
  const calls = () => readFileSync(logPath, "utf8").split("\n").filter(Boolean);
  const run = async (args: string[], extra: Record<string, string> = {}) => {
    const child = Bun.spawn(["bash", SCRIPT, ...args], { env: { ...env, ...extra }, stdout: "pipe", stderr: "pipe" });
    const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
    expect(calls().filter((call) => call.startsWith("FORBIDDEN") || call.includes("funnel"))).toEqual([]);
    return { code, out, err };
  };
  return {
    root,
    state,
    run,
    calls,
    clearCalls: () => writeFileSync(logPath, ""),
    live: (): Serve => JSON.parse(readFileSync(configPath, "utf8")).serve,
    setLive: (next: Serve) => writeFileSync(configPath, JSON.stringify({ node: NODE, serve: next })),
    record: () => JSON.parse(readFileSync(state, "utf8")),
    hasState: () => existsSync(state),
  };
}

const ownedAll = Object.fromEntries(PORTS.map((port) => [String(port), { host: NODE, backend: BACKEND }]));

describe("up", () => {
  test("routes the host and each product port, and records exactly those", async () => {
    const t = setup();
    expect((await t.run(["up"])).code).toBe(0);
    expect(t.live()).toEqual(routes(PORTS));
    expect(t.record().owned).toEqual(ownedAll);
  });

  test("is idempotent: a repeat run changes nothing and keeps the first snapshot", async () => {
    const unrelated = routes([8443], OTHER_BACKEND);
    const t = setup(unrelated);
    await t.run(["up"]);
    const first = readFileSync(t.state, "utf8");
    t.clearCalls();
    expect((await t.run(["up"])).code).toBe(0);
    expect(t.calls()).toEqual([]);
    expect(readFileSync(t.state, "utf8")).toBe(first);
    expect(t.record().snapshot).toEqual(unrelated);
  });

  test("a later run that adds ports keeps the original snapshot", async () => {
    const original = routes([8443], OTHER_BACKEND);
    const t = setup(original);
    await t.run(["up"]);
    await t.run(["up"], { PORTS: "9450-9452" });
    expect(t.record().snapshot).toEqual(original);
    expect(Object.keys(t.record().owned).sort()).toEqual(["443", "9450", "9451", "9452"]);
  });

  test("a route that already points at the backend is left alone, and down leaves it", async () => {
    const t = setup(routes([9451]));
    const { code, out } = await t.run(["up"]);
    expect(code).toBe(0);
    expect(out).toContain("port 9451: already points at");
    expect(t.calls().some((call) => call.includes("--https=9451"))).toBe(false);
    expect(Object.keys(t.record().owned).sort()).toEqual(["443", "9450"]);
    await t.run(["down"]);
    expect(t.live()).toEqual(routes([9451]));
  });

  const conflicts: Record<string, Serve> = {
    "another target": routes([9451], OTHER_BACKEND),
    "an extra handler": {
      TCP: { "9451": { HTTPS: true } },
      Web: { [`${NODE}:9451`]: { Handlers: { "/": { Proxy: BACKEND }, "/api": { Proxy: OTHER_BACKEND } } } },
    },
    "a non-proxy handler": {
      TCP: { "9451": { HTTPS: true } },
      Web: { [`${NODE}:9451`]: { Handlers: { "/": { Text: "hello" } } } },
    },
    "the same target under another host name": {
      TCP: { "9451": { HTTPS: true } },
      Web: { "other.tailnet.example:9451": { Handlers: { "/": { Proxy: BACKEND } } } },
    },
    "a TCP forward": { TCP: { "9451": { TCPForward: "127.0.0.1:22" } } },
    "Funnel already on": { ...routes([9451]), AllowFunnel: { [`${NODE}:9451`]: true } },
  };
  for (const [name, serve] of Object.entries(conflicts)) {
    test(`refuses without changing anything when a port has ${name}`, async () => {
      const t = setup(serve);
      const { code, err } = await t.run(["up"]);
      expect(code).toBe(1);
      expect(err).toContain("port 9451");
      expect(t.calls()).toEqual([]);
      expect(t.live()).toEqual(serve);
      expect(t.hasState()).toBe(false);
    });
  }

  test("a failure part way undoes the routes made in this run", async () => {
    const unrelated = routes([8443], OTHER_BACKEND);
    const t = setup(unrelated);
    const { code, err } = await t.run(["up"], { FAKE_TS_FAIL_SERVE_PORT: "9451" });
    expect(code).toBe(1);
    expect(err).toContain("port 9451");
    expect(t.live()).toEqual(unrelated);
    expect(t.hasState()).toBe(false);
  });

  test("a failure on a repeat run keeps the routes from the earlier run", async () => {
    const t = setup();
    await t.run(["up"], { PORTS: "9450-9450" });
    await t.run(["up"], { FAKE_TS_FAIL_SERVE_PORT: "9451" });
    expect(t.live()).toEqual(routes([443, 9450]));
    expect(Object.keys(t.record().owned).sort()).toEqual(["443", "9450"]);
  });

  test("what could not be undone stays recorded, and a later down finishes it", async () => {
    const t = setup();
    const failed = await t.run(["up"], { FAKE_TS_FAIL_SERVE_PORT: "9451", FAKE_TS_FAIL_OFF_PORT: "443" });
    expect(failed.code).toBe(1);
    expect(t.record().owned).toEqual({ "443": { host: NODE, backend: BACKEND } });
    expect((await t.run(["down"])).code).toBe(0);
    expect(t.live()).toEqual({ TCP: {}, Web: {} });
    expect(t.hasState()).toBe(false);
  });

  test("a leftover lock blocks up and down and names its path", async () => {
    const t = setup();
    mkdirSync(`${t.state}.lock`, { recursive: true });
    for (const command of ["up", "down"]) {
      const { code, err } = await t.run([command]);
      expect(code).toBe(1);
      expect(err).toContain(`${t.state}.lock`);
    }
    expect(t.calls()).toEqual([]);
  });
});

describe("settings and record", () => {
  const invalid: [string, Record<string, string>][] = [
    ["a backend that is not loopback http", { BACKEND: "http://192.0.2.10:5181" }],
    ["a backend with shell syntax", { BACKEND: "http://127.0.0.1:5181; touch pwned" }],
    ["a range holding 443", { PORTS: "400-500" }],
    ["a descending range", { PORTS: "9459-9450" }],
    ["a range with shell syntax", { PORTS: "9450-9451; touch pwned" }],
  ];
  for (const [name, extra] of invalid) {
    test(`up rejects ${name}`, async () => {
      const t = setup();
      expect((await t.run(["up"], extra)).code).toBe(1);
      expect(t.calls()).toEqual([]);
      expect(existsSync(join(t.root, "pwned"))).toBe(false);
    });
  }

  test("a route under a different host name is not ours, so down leaves it", async () => {
    const t = setup();
    await t.run(["up"], { PORTS: "9450-9450" });
    const live = t.live();
    live.Web![`renamed.tailnet.example:9450`] = live.Web![`${NODE}:9450`];
    delete live.Web![`${NODE}:9450`];
    t.setLive(live);
    t.clearCalls();
    const { code, out } = await t.run(["down"]);
    expect(code).toBe(0);
    expect(out).toContain("port 9450: left alone");
    expect(out).toContain("removed port 443");
    expect(t.calls()).toEqual(["serve --https=443 off"]);
    expect(Object.keys(t.live().Web!)).toEqual(["renamed.tailnet.example:9450"]);
  });

  const malformed: [string, unknown][] = [
    ["a null backend", { "9450": { host: NODE, backend: null } }],
    ["a missing host", { "9450": { backend: BACKEND } }],
    ["an old-format entry", { "9450": BACKEND }],
    ["a non-numeric port", { "94x0": { host: NODE, backend: BACKEND } }],
  ];
  for (const [name, owned] of malformed) {
    test(`a record with ${name} stops up and down before any change`, async () => {
      const t = setup(routes([9450]));
      mkdirSync(join(t.root, "state"));
      const record = JSON.stringify({ owned, snapshot: {} });
      writeFileSync(t.state, record);
      for (const command of ["up", "down"]) {
        const { code, err } = await t.run([command]);
        expect(code).toBe(1);
        expect(err).toContain("delete the file");
      }
      expect(readFileSync(t.state, "utf8")).toBe(record);
      expect(t.calls()).toEqual([]);
      expect(t.live()).toEqual(routes([9450]));
    });
  }

  test("a file that is not a record is never overwritten", async () => {
    const t = setup();
    mkdirSync(join(t.root, "state"));
    writeFileSync(t.state, "precious notes\n");
    for (const command of ["up", "down"]) expect((await t.run([command])).code).toBe(1);
    expect(readFileSync(t.state, "utf8")).toBe("precious notes\n");
    expect(t.calls()).toEqual([]);
  });
});

describe("down", () => {
  test("removes exactly the routes up made and leaves unrelated routes and handlers", async () => {
    const unrelated: Serve = {
      TCP: { "8443": { HTTPS: true }, "9459": { HTTPS: true } },
      Web: {
        [`${NODE}:8443`]: { Handlers: { "/": { Proxy: OTHER_BACKEND }, "/files": { Path: "/srv" } } },
        [`${NODE}:9459`]: { Handlers: { "/": { Proxy: BACKEND } } },
      },
    };
    const t = setup(unrelated);
    await t.run(["up"]);
    expect((await t.run(["down"])).code).toBe(0);
    expect(t.live()).toEqual(unrelated);
    expect(t.hasState()).toBe(false);
  });

  test("with no record it changes nothing, even if routes point at the backend", async () => {
    const t = setup(routes(PORTS));
    const { code, out } = await t.run(["down"]);
    expect(code).toBe(0);
    expect(out).toContain("changed nothing");
    expect(t.calls()).toEqual([]);
    expect(t.live()).toEqual(routes(PORTS));
  });

  test("leaves a route that was edited since up, and still removes the rest", async () => {
    const t = setup();
    await t.run(["up"]);
    const live = t.live();
    (live.Web![`${NODE}:9450`] as { Handlers: Record<string, unknown> }).Handlers["/extra"] = { Text: "x" };
    t.setLive(live);
    const { code, out } = await t.run(["down"]);
    expect(code).toBe(0);
    expect(out).toContain("port 9450: left alone");
    expect(Object.keys(t.live().TCP!)).toEqual(["9450"]);
    expect(t.hasState()).toBe(false);
  });

  test("reports a route that is already gone", async () => {
    const t = setup();
    await t.run(["up"]);
    t.setLive({ TCP: {}, Web: {} });
    const { code, out } = await t.run(["down"]);
    expect(code).toBe(0);
    expect(out).toContain("already gone");
  });

  test("a removal that fails stays recorded, and a retry finishes it", async () => {
    const t = setup();
    await t.run(["up"]);
    const failed = await t.run(["down"], { FAKE_TS_FAIL_OFF_PORT: "9451" });
    expect(failed.code).toBe(1);
    expect(Object.keys(t.record().owned)).toEqual(["9451"]);
    expect((await t.run(["down"])).code).toBe(0);
    expect(t.live()).toEqual({ TCP: {}, Web: {} });
  });

  test("a repeated down is harmless, and a fresh up takes a new snapshot", async () => {
    const t = setup();
    await t.run(["up"]);
    await t.run(["down"]);
    t.clearCalls();
    expect((await t.run(["down"])).code).toBe(0);
    expect(t.calls()).toEqual([]);
    const later = routes([8443], OTHER_BACKEND);
    t.setLive(later);
    await t.run(["up"]);
    expect(t.record().snapshot).toEqual(later);
  });
});

describe("status", () => {
  test("shows what the script made", async () => {
    const t = setup();
    expect((await t.run(["status"])).out).toContain("owned by this script: nothing");
    await t.run(["up"]);
    expect((await t.run(["status"])).out).toContain(`${NODE}:443 -> ${BACKEND}`);
  });
});
