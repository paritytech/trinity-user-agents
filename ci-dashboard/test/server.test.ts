import { describe, expect, test } from "bun:test";
import { createHmac } from "node:crypto";
import { mkdtemp, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createApp, MAX_BODY_BYTES, serveSite } from "../server/app";
import { fold, post, queue, type Message } from "../server/matrix";
import { Monitor, type Checks, type Status } from "../server/monitor";
import { githubEvent, sentryEvent, verifyGithub } from "../server/webhooks";
import config from "../src/config";

const SECRET = "test-secret";
const sign = (body: string) =>
  createHmac("sha256", SECRET).update(body).digest("hex");

const run = (overrides: Record<string, unknown> = {}) => ({
  action: "completed",
  workflow_run: {
    name: "Android nightly",
    display_title: "Android nightly",
    path: ".github/workflows/android-nightly.yml",
    conclusion: "failure",
    event: "schedule",
    head_branch: "main",
    html_url: "https://github.test/run/1",
    ...overrides,
  },
});

describe("verifyGithub", () => {
  test("accepts the right signature and refuses others", () => {
    const body = '{"a":1}';
    expect(verifyGithub(SECRET, body, `sha256=${sign(body)}`)).toBe(true);
    expect(verifyGithub(SECRET, body, `sha256=${sign("{}")}`)).toBe(false);
    expect(verifyGithub(SECRET, body, sign(body))).toBe(false);
    expect(verifyGithub(SECRET, body, null)).toBe(false);
    // An unset secret refuses everything.
    expect(verifyGithub("", body, `sha256=${sign(body)}`)).toBe(false);
  });
});

describe("githubEvent", () => {
  test("posts a failed scheduled run of a watched workflow", () => {
    const outcome = githubEvent("workflow_run", run(), config);
    expect(outcome.refresh).toBe(true);
    expect(outcome.message?.text).toContain("❌ Android nightly failed");
  });

  test("posts the main iOS night but not a branch night", () => {
    const ios = (display_title: string) =>
      run({
        name: "iOS Nightly",
        display_title,
        path: ".github/workflows/ios-nightly-prepare.yml",
      });
    expect(
      githubEvent("workflow_run", ios("iOS Nightly (main)"), config).message,
    ).toBeDefined();
    expect(
      githubEvent("workflow_run", ios("iOS Nightly (release/1.2)"), config),
    ).toEqual({ refresh: false });
  });

  test("posts Release failures although the page does not list it", () => {
    const outcome = githubEvent(
      "workflow_run",
      run({ name: "Release", path: ".github/workflows/release.yml" }),
      config,
    );
    expect(outcome.message).toBeDefined();
  });

  test("stays quiet for passes, pull requests and unwatched workflows", () => {
    for (const overrides of [
      { conclusion: "success" },
      { event: "pull_request" },
      { path: ".github/workflows/ci.yml" },
    ]) {
      expect(
        githubEvent("workflow_run", run(overrides), config).message,
      ).toBeUndefined();
    }
  });

  test("announces releases but not prereleases", () => {
    const release = (prerelease: boolean) => ({
      action: "published",
      release: {
        name: "@parity/truapi 1.2.0",
        tag_name: "@parity/truapi@1.2.0",
        html_url: "https://github.test/release",
        prerelease,
        draft: false,
      },
    });
    expect(
      githubEvent("release", release(false), config).message?.text,
    ).toContain("📦 @parity/truapi 1.2.0 released");
    const nightly = githubEvent("release", release(true), config);
    expect(nightly).toEqual({ refresh: true });
  });
});

describe("packages", () => {
  const published = (version: string, action = "published") => ({
    action,
    package: {
      name: "io.parity.truapi-host-android",
      package_type: "maven",
      html_url: "https://github.test/package",
      package_version: { version },
    },
  });

  test("announces a published release version", () => {
    const outcome = githubEvent("package", published("0.26.0"), config);
    expect(outcome.message?.text).toBe(
      "📦 io.parity.truapi-host-android 0.26.0 published to GitHub Packages (maven) https://github.test/package",
    );
  });

  test("leaves out snapshots, pre-releases and other actions", () => {
    for (const payload of [
      published("0.27.0-SNAPSHOT"),
      published("0.27.0-rc.1"),
      published("0.26.0", "updated"),
    ]) {
      expect(githubEvent("package", payload, config).message).toBeUndefined();
    }
  });
});

describe("sentryEvent", () => {
  const issue = (slug: string, action = "created") => ({
    action,
    data: {
      issue: {
        shortId: "IOS-1",
        title: "Crash <here>",
        level: "error",
        web_url: "https://sentry.test/1",
        project: { slug },
      },
    },
  });

  test("posts new issues of configured projects", () => {
    const slug = config.sentry.projects[0]!.slug;
    const message = sentryEvent("issue", issue(slug), config);
    expect(message?.html).toContain("Crash &lt;here&gt;");
  });

  test("ignores other projects, actions and resources", () => {
    const slug = config.sentry.projects[0]!.slug;
    expect(sentryEvent("issue", issue("other"), config)).toBeUndefined();
    expect(
      sentryEvent("issue", issue(slug, "resolved"), config),
    ).toBeUndefined();
    expect(sentryEvent("error", issue(slug), config)).toBeUndefined();
  });
});

describe("post", () => {
  const relay = {
    url: "https://relay.test/",
    token: "fake-relay-token",
    rooms: ["!a:parity.io", "!b:parity.io"],
  };

  test("sends to every room through the relay and keeps going when one fails", async () => {
    const calls: { url: string; init: RequestInit }[] = [];
    const fetchFn = (async (url: string, init: RequestInit) => {
      calls.push({ url, init });
      // The relay answers 403 for a room that is not on its allowlist.
      return new Response(null, {
        status: url.endsWith(encodeURIComponent("!b:parity.io")) ? 403 : 204,
      });
    }) as typeof fetch;
    const results = await post(relay, { text: "t", html: "h" }, fetchFn);
    expect(results.map((r) => r.ok)).toEqual([true, false]);
    expect(calls[0]!.url).toBe("https://relay.test/matrix/send/!a%3Aparity.io");
    expect(calls[0]!.init.method).toBe("POST");
    const headers = calls[0]!.init.headers as Record<string, string>;
    expect(headers.authorization).toBe("fake-relay-token");
    expect(JSON.parse(String(calls[0]!.init.body))).toEqual({
      text: "t",
      formatted_text: "h",
    });
  });

  test("waits as long as the relay asks after a rate limit, once", async () => {
    let count = 0;
    const fetchFn = (async () =>
      ++count === 1
        ? new Response(null, { status: 429, headers: { "retry-after": "0" } })
        : new Response(null, { status: 204 })) as unknown as typeof fetch;
    const [result] = await post(
      { ...relay, rooms: ["!a:parity.io"] },
      { text: "t", html: "h" },
      fetchFn,
    );
    expect(result?.ok).toBe(true);
    expect(count).toBe(2);
  });
});

describe("Monitor", () => {
  async function smoke(dir: string, body: string) {
    const script = join(dir, "smoke.sh");
    await writeFile(script, body);
    return script;
  }

  test("keeps the smoke rows and posts only when the verdict changes", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ci-dashboard-"));
    const failing = await smoke(
      dir,
      "echo '| ✅ | CLI | 1.0 |'\necho '| ❌ | npm | broken |'\nexit 1\n",
    );
    const sent: Message[] = [];
    const monitor = new Monitor(
      config,
      dir,
      async (m) => void sent.push(m),
      "https://dashboard.test",
    );
    await monitor.runSmoke(failing);
    expect(monitor.status.smoke?.ok).toBe(false);
    expect(monitor.status.smoke?.rows).toEqual([
      { ok: true, name: "CLI", detail: "1.0" },
      { ok: false, name: "npm", detail: "broken" },
    ]);
    expect(sent).toHaveLength(1);
    expect(sent[0]?.text).toContain("npm: broken");

    // Still failing the next morning: nothing new to say.
    await monitor.runSmoke(failing);
    expect(sent).toHaveLength(1);

    // A restart reads back what was saved, so it does not post again either.
    const again = new Monitor(
      config,
      dir,
      async (m) => void sent.push(m),
      "https://dashboard.test",
    );
    await again.load();
    expect(again.status.smoke?.rows).toHaveLength(2);
    await again.runSmoke(failing);
    expect(sent).toHaveLength(1);

    await again.runSmoke(await smoke(dir, "echo '| ✅ | npm | ok |'\n"));
    expect(sent).toHaveLength(2);
    expect(sent[1]?.text).toContain("pass again");
  });

  test("reports a script that fails before its table", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ci-dashboard-"));
    const sent: Message[] = [];
    const monitor = new Monitor(
      config,
      dir,
      async (m) => void sent.push(m),
      "",
    );
    await monitor.runSmoke(
      await smoke(dir, "echo 'jq: not found' >&2\nexit 127\n"),
    );
    expect(monitor.status.smoke?.output).toContain("jq: not found");
    expect(sent[0]?.text).toContain("smoke.sh exited with 127");
  });
});

describe("Monitor smoke limits", () => {
  test("gives the script no secret", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ci-dashboard-"));
    process.env.FAKE_DASHBOARD_SECRET = "fake-secret";
    try {
      const monitor = new Monitor(config, dir, async () => {}, "");
      await monitor.runSmoke(
        await smoke(dir, 'echo "secret=${FAKE_DASHBOARD_SECRET:-none}"\n'),
      );
      expect(monitor.status.smoke?.output).toContain("secret=none");
    } finally {
      delete process.env.FAKE_DASHBOARD_SECRET;
    }
  });

  test("stops a run that hangs and reports it", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ci-dashboard-"));
    const sent: Message[] = [];
    const monitor = new Monitor(
      config,
      dir,
      async (m) => void sent.push(m),
      "",
    );
    await monitor.runSmoke(await smoke(dir, "sleep 30\n"), 200);
    expect(monitor.status.smoke?.ok).toBe(false);
    expect(sent[0]?.text).toContain("did not finish");
  });

  async function smoke(dir: string, body: string) {
    const script = join(dir, "smoke.sh");
    await writeFile(script, body);
    return script;
  }
});

describe("Monitor checks", () => {
  const ok = { ok: true, detail: "fine" };
  const checks = (certificateOk: () => boolean): Checks => ({
    rpc: async (e) => ({ name: e.name, url: e.url, ...ok }),
    link: async (l) => ({ name: l.name, url: l.url, ...ok }),
    certificate: async (host) => ({
      name: `${host} certificate`,
      url: `tls://${host}`,
      ok: certificateOk(),
      detail: certificateOk() ? "expires in 60 days" : "expires in 3 days",
    }),
  });

  test("keeps certificate failures counted across the minute checks", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ci-dashboard-"));
    const sent: Message[] = [];
    const monitor = new Monitor(
      config,
      dir,
      async (m) => void sent.push(m),
      "",
      checks(() => false),
    );
    await monitor.checkCertificates();
    // The minute checks run in between and must not reset the count.
    await monitor.checkEndpoints();
    await monitor.checkCertificates();
    expect(sent).toHaveLength(1);
    expect(sent[0]?.text).toContain("certificate is down");
  });

  test("saves through a temporary file, so checks at once leave a whole file", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ci-dashboard-"));
    const monitor = new Monitor(
      config,
      dir,
      async () => {},
      "",
      checks(() => true),
    );
    await Promise.all([monitor.checkEndpoints(), monitor.checkCertificates()]);
    expect(await readdir(dir)).toEqual(["monitor.json"]);
    const again = new Monitor(config, dir, async () => {}, "");
    await again.load();
    expect(again.status.rpc).toHaveLength(config.rpcEndpoints.length);
    expect(again.status.certificates.length).toBeGreaterThan(0);
  });
});

describe("queue", () => {
  test("folds a burst into one post, one post at a time", async () => {
    const posts: Message[] = [];
    let inFlight = 0;
    let most = 0;
    const notify = queue(async (m) => {
      inFlight++;
      most = Math.max(most, inFlight);
      await Bun.sleep(5);
      posts.push(m);
      inFlight--;
    }, 10);
    const burst = Array.from({ length: 5 }, (_, i) => ({
      text: `issue ${i}`,
      html: `<b>issue ${i}</b>`,
    }));
    await Promise.all(burst.map(notify));
    expect(posts).toHaveLength(1);
    expect(posts[0]!.text).toBe(burst.map((m) => m.text).join("\n\n"));
    expect(most).toBe(1);

    // Later messages start a new post.
    await notify({ text: "later", html: "later" });
    expect(posts.map((p) => p.text).at(-1)).toBe("later");
  });

  test("counts what does not fit", () => {
    const many = Array.from({ length: 25 }, (_, i) => ({
      text: `${i}`,
      html: `${i}`,
    }));
    expect(fold(many).text.endsWith("…and 5 more")).toBe(true);
  });
});

describe("createApp", () => {
  const status: Status = { rpc: [], links: [], certificates: [] };

  async function setUp() {
    const siteDir = await mkdtemp(join(tmpdir(), "ci-dashboard-site-"));
    await writeFile(join(siteDir, "index.html"), "<p>page</p>");
    const sent: Message[] = [];
    let refreshed = 0;
    const app = createApp({
      config,
      siteDir,
      snapshot: () => undefined,
      refresh: () => void refreshed++,
      status: () => status,
      notify: async (m) => void sent.push(m),
      githubWebhookSecret: SECRET,
      sentryClientSecret: SECRET,
    });
    return { app, sent, refreshed: () => refreshed };
  }

  const request = (path: string, init?: RequestInit) =>
    new Request(`http://localhost${path}`, init);

  test("serves the page, its data and nothing outside the site", async () => {
    const { app } = await setUp();
    expect(await (await app(request("/"))).text()).toBe("<p>page</p>");
    expect(await (await app(request("/up"))).text()).toBe("ok");
    expect(await (await app(request("/api/dashboard"))).json()).toEqual({
      github: null,
      checks: status,
    });
  });

  test("refuses a path that leaves the site directory", async () => {
    // The URL parser already resolves `..` in a request, so the guard is
    // tested on the path it is given.
    const siteDir = await mkdtemp(join(tmpdir(), "ci-dashboard-site-"));
    await writeFile(join(siteDir, "index.html"), "<p>page</p>");
    await writeFile(join(siteDir, "..", "outside.txt"), "secret");
    expect((await serveSite(siteDir, "/../outside.txt")).status).toBe(404);
    expect((await serveSite(siteDir, "/index.html")).status).toBe(200);
  });

  test("acts on a signed GitHub webhook and refuses an unsigned one", async () => {
    const { app, sent, refreshed } = await setUp();
    const body = JSON.stringify(run());
    const unsigned = await app(
      request("/webhooks/github", {
        method: "POST",
        body,
        headers: { "x-github-event": "workflow_run" },
      }),
    );
    expect(unsigned.status).toBe(401);
    expect(sent).toHaveLength(0);

    const signed = await app(
      request("/webhooks/github", {
        method: "POST",
        body,
        headers: {
          "x-github-event": "workflow_run",
          "x-hub-signature-256": `sha256=${sign(body)}`,
        },
      }),
    );
    expect(signed.status).toBe(200);
    expect(sent).toHaveLength(1);
    expect(refreshed()).toBe(1);
  });

  test("acts on a signed Sentry webhook", async () => {
    const { app, sent } = await setUp();
    const body = JSON.stringify({
      action: "created",
      data: {
        issue: {
          shortId: "IOS-2",
          title: "Crash",
          level: "error",
          web_url: "https://sentry.test/2",
          project: { slug: config.sentry.projects[0]!.slug },
        },
      },
    });
    const response = await app(
      request("/webhooks/sentry", {
        method: "POST",
        body,
        headers: {
          "sentry-hook-resource": "issue",
          "sentry-hook-signature": sign(body),
        },
      }),
    );
    expect(response.status).toBe(200);
    expect(sent[0]?.text).toContain("IOS-2");
  });
  test("answers bad JSON with 400, even when signed", async () => {
    const { app, sent } = await setUp();
    const body = "{not json";
    const response = await app(
      request("/webhooks/github", {
        method: "POST",
        body,
        headers: {
          "x-github-event": "workflow_run",
          "x-hub-signature-256": `sha256=${sign(body)}`,
        },
      }),
    );
    expect(response.status).toBe(400);
    expect(sent).toHaveLength(0);
  });

  test("answers before the post is sent", async () => {
    const siteDir = await mkdtemp(join(tmpdir(), "ci-dashboard-site-"));
    let called = 0;
    const app = createApp({
      config,
      siteDir,
      snapshot: () => undefined,
      refresh: () => {},
      status: () => status,
      // A relay that never answers.
      notify: () => {
        called++;
        return new Promise(() => {});
      },
      githubWebhookSecret: SECRET,
      sentryClientSecret: SECRET,
    });
    const body = JSON.stringify(run());
    const response = await app(
      request("/webhooks/github", {
        method: "POST",
        body,
        headers: {
          "x-github-event": "workflow_run",
          "x-hub-signature-256": `sha256=${sign(body)}`,
        },
      }),
    );
    expect(response.status).toBe(200);
    expect(called).toBe(1);
  });

  test("refuses a body over the cap before reading it whole", async () => {
    const { app } = await setUp();
    const server = Bun.serve({
      port: 0,
      maxRequestBodySize: MAX_BODY_BYTES,
      fetch: app,
    });
    try {
      const response = await fetch(
        `http://localhost:${server.port}/webhooks/github`,
        { method: "POST", body: "x".repeat(MAX_BODY_BYTES + 1) },
      );
      expect(response.status).toBe(413);
    } finally {
      server.stop(true);
    }
  });
});
