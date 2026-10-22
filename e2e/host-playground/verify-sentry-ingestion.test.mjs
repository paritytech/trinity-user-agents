import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { resolveAndroidDsn, verifySentryIngestion } from "./verify-sentry-ingestion.mjs";

const sessionId = "123e4567-e89b-42d3-a456-426614174000";
const marker = "native-telemetry-e2e-deadbeef";
const eventId = "abcdef0123456789abcdef0123456789";
const nativeEventId = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const logEventId = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const token = "never-print-this-api-token";
const dsn = "https://public-key@o1.ingest.de.sentry.io/100";
const bootstrap = {
  hostSessionId: sessionId,
  appVariant: "nightly",
  platform: "android",
  appVersion: "1.2.3",
  appBuild: "456",
  truapiHostVersion: "0.9.0",
};
const projects = [
  { id: "100", slug: "polkadot-android" },
  { id: "200", slug: "jollity-next" },
];

function nativeDetail() {
  return {
    id: nativeEventId,
    message: "host.webview.created",
    tags: [
      { key: "host.session_id", value: sessionId },
      { key: "host.app_variant", value: bootstrap.appVariant },
      { key: "host.platform", value: "android" },
      { key: "host.native_app_version", value: bootstrap.appVersion },
      { key: "truapi.host_version", value: bootstrap.truapiHostVersion },
    ],
    context: { "host.app_build": 456, "event.monotonic_ms": 123.5 },
  };
}

function jollityDetail() {
  return {
    eventID: eventId,
    exception: { values: [{ value: marker }] },
    tags: [{ key: "host.session_id", value: sessionId }],
    contexts: { native_host: { hostSessionId: sessionId } },
    breadcrumbs: [{ category: "replay", message: "session.link", data: { data: JSON.stringify({ observation: { hostSessionId: sessionId } }) } }],
  };
}

function json(payload, status = 200, headers = {}) {
  return new Response(JSON.stringify(payload), { status, headers });
}

function setup(t) {
  const dir = mkdtempSync(join(tmpdir(), "sentry-acceptance-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const evidencePath = join(dir, "evidence.json");
  const calls = [];
  const fetchImpl = async (input) => {
    const url = new URL(input);
    calls.push(url);
    if (url.pathname.endsWith("/projects/")) return json(projects);
    if (url.pathname.endsWith("/keys/")) {
      return json([{ isActive: true, projectId: 100, dsn: { public: dsn } }]);
    }
    if (url.pathname.endsWith("/events/")) {
      const projectId = url.searchParams.get("project");
      assert.match(projectId, /^\d+$/);
      assert.equal(url.searchParams.get("query"), `host.session_id:${sessionId}`);
      const dataset = url.searchParams.get("dataset");
      const fields = url.searchParams.getAll("field");
      assert.equal(fields.includes("tag[host.session_id,string]"), false);
      if (dataset === "logs") {
        assert.deepEqual(fields, ["id", "timestamp", "message", "host.app_build", "host.native_app_version", "truapi.host_version"]);
        return json({ data: [{ id: logEventId, timestamp: "2026-10-21T05:00:00Z", message: "previous-session-end", "host.app_build": "456", "host.native_app_version": "1.2.3", "truapi.host_version": "0.9.0" }] });
      }
      if (projectId === "100") return json({ data: [{ id: nativeEventId, timestamp: "2026-10-21T05:00:00Z", message: "host.webview.created" }] });
      return json({ data: [{ id: eventId, timestamp: "2026-10-21T05:00:01Z", message: marker }] });
    }
    if (url.pathname.endsWith(`/${nativeEventId}/`)) {
      return json(nativeDetail());
    }
    if (url.pathname.endsWith(`/${eventId}/`)) {
      return json(jollityDetail());
    }
    throw new Error(`unexpected fixture URL: ${url.pathname}`);
  };
  return { evidencePath, calls, fetchImpl };
}

function replaceEventDetail(fixture, wantedEventId, mutate) {
  const defaultFetch = fixture.fetchImpl;
  fixture.fetchImpl = async (input) => {
    const url = new URL(input);
    if (!url.pathname.endsWith(`/${wantedEventId}/`)) return defaultFetch(input);
    const detail = wantedEventId === nativeEventId ? nativeDetail() : jollityDetail();
    mutate(detail);
    return json(detail);
  };
}

function verifyOptions(setupResult, overrides = {}) {
  return {
    apiBase: "https://de.sentry.io",
    org: "paritytech",
    nativeProject: "polkadot-android",
    jollityProject: "jollity-next",
    token,
    sessionId,
    marker,
    jollityEventId: eventId,
    bootstrap,
    evidencePath: setupResult.evidencePath,
    candidateRef: "a".repeat(40),
    harnessSha: "b".repeat(40),
    fetchImpl: setupResult.fetchImpl,
    sleep: async () => {},
    pollAttempts: 1,
    pollIntervalMs: 0,
    ...overrides,
  };
}

test("queries numeric project IDs and verifies the exact backend events against one UUID", async (t) => {
  const fixture = setup(t);
  const result = await verifySentryIngestion(verifyOptions(fixture));
  assert.equal(result.verified, true);
  assert.equal(result.projects.native.id, "100");
  assert.equal(result.projects.jollity.markedEvent.eventId, eventId);
  assert.equal(result.projects.native.event.eventId, nativeEventId);
  assert.ok(fixture.calls.some((url) => url.searchParams.get("project") === "100"));
  assert.ok(fixture.calls.some((url) => url.searchParams.get("project") === "200"));
  assert.ok(fixture.calls.some((url) => url.pathname.endsWith(`/projects/paritytech/200/events/${eventId}/`)));
  const saved = readFileSync(fixture.evidencePath, "utf8");
  assert.equal(saved.includes(token), false);
  assert.equal(saved.includes(dsn), false);
  assert.equal(saved.includes(sessionId), true);
});

test("native event requires finite context extras and exact platform and TruAPI tags", async (t) => {
  const cases = [
    ["host.app_build", null, /host\.app_build extra is not a finite number/],
    ["host.app_build", 457, /host\.app_build extra did not match/],
    ["event.monotonic_ms", Number.NaN, /event\.monotonic_ms extra is not a finite number/],
  ];
  for (const [key, value, expected] of cases) {
    const fixture = setup(t);
    const base = setup(t);
    fixture.fetchImpl = async (input) => {
      const url = new URL(input);
      if (url.pathname.endsWith(`/projects/paritytech/100/events/${nativeEventId}/`)) {
        return json({
          id: nativeEventId,
          message: "host.webview.created",
          tags: [
            { key: "host.session_id", value: sessionId },
            { key: "host.app_variant", value: bootstrap.appVariant },
            { key: "host.platform", value: "android" },
            { key: "host.native_app_version", value: bootstrap.appVersion },
            { key: "truapi.host_version", value: bootstrap.truapiHostVersion },
          ],
          context: { "host.app_build": 456, "event.monotonic_ms": 123.5, [key]: value },
        });
      }
      return base.fetchImpl(input);
    };
    await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), expected);
  }
  const fixture = setup(t);
  const base = setup(t);
  fixture.fetchImpl = async (input) => {
    const url = new URL(input);
    if (url.pathname.endsWith(`/projects/paritytech/100/events/${nativeEventId}/`)) {
      return json({
        id: nativeEventId,
        message: "host.webview.created",
        tags: [
          { key: "host.session_id", value: sessionId },
          { key: "host.app_variant", value: bootstrap.appVariant },
          { key: "host.platform", value: "ios" },
          { key: "host.native_app_version", value: bootstrap.appVersion },
          { key: "truapi.host_version", value: bootstrap.truapiHostVersion },
        ],
        context: { "host.app_build": 456, "event.monotonic_ms": 123.5 },
      });
    }
    return base.fetchImpl(input);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), /native platform tag did not match/);
  const truapiFixture = setup(t);
  const truapiBase = truapiFixture.fetchImpl;
  truapiFixture.fetchImpl = async (input) => {
    const url = new URL(input);
    if (url.pathname.endsWith(`/projects/paritytech/100/events/${nativeEventId}/`)) {
      return json({
        id: nativeEventId,
        message: "host.webview.created",
        tags: [
          { key: "host.session_id", value: sessionId },
          { key: "host.app_variant", value: bootstrap.appVariant },
          { key: "host.platform", value: "android" },
          { key: "host.native_app_version", value: bootstrap.appVersion },
          { key: "truapi.host_version", value: "0.8.0" },
        ],
        context: { "host.app_build": 456, "event.monotonic_ms": 123.5 },
      });
    }
    return truapiBase(input);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(truapiFixture)), /native TruAPI host version tag did not match/);
});

test("every native event identity and metadata check fails closed", async (t) => {
  const cases = [
    ["event ID", (event) => { event.id = "c".repeat(32); }, /native event ID/],
    ["message", (event) => { event.message = "host.webview.closed"; }, /native event message/],
    ["session", (event) => { event.tags[0].value = "623e4567-e89b-42d3-a456-426614174000"; }, /native host\.session_id tag/],
    ["variant", (event) => { event.tags[1].value = "release"; }, /native app variant tag/],
    ["version", (event) => { event.tags[3].value = "9.9.9"; }, /native app version tag/],
  ];
  for (const [, mutate, expected] of cases) {
    const fixture = setup(t);
    replaceEventDetail(fixture, nativeEventId, mutate);
    await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), expected);
  }
});

test("every Jollity event join check fails closed", async (t) => {
  const cases = [
    ["event ID", (event) => { event.eventID = "c".repeat(32); }, /Jollity event ID/],
    ["marker", (event) => { event.exception.values[0].value = "different marker"; }, /Jollity marker/],
    ["tag", (event) => { event.tags[0].value = "623e4567-e89b-42d3-a456-426614174000"; }, /Jollity host\.session_id tag/],
    ["context", (event) => { event.contexts.native_host.hostSessionId = "623e4567-e89b-42d3-a456-426614174000"; }, /Jollity native host session context/],
    ["breadcrumb", (event) => { event.breadcrumbs[0].data.data = JSON.stringify({ observation: { hostSessionId: "623e4567-e89b-42d3-a456-426614174000" } }); }, /Jollity session\.link breadcrumb/],
  ];
  for (const [, mutate, expected] of cases) {
    const fixture = setup(t);
    replaceEventDetail(fixture, eventId, mutate);
    await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), expected);
  }
});

test("previous-session-end log fields must match captured build and version metadata", async (t) => {
  for (const changed of ["host.app_build", "host.native_app_version", "truapi.host_version"]) {
    const fixture = setup(t);
    const defaultFetch = fixture.fetchImpl;
    fixture.fetchImpl = async (input) => {
      const url = new URL(input);
      if (url.searchParams.get("dataset") === "logs") {
        return json({ data: [{ id: logEventId, timestamp: "2026-10-21T05:00:00Z", message: "previous-session-end", "host.app_build": "456", "host.native_app_version": "1.2.3", "truapi.host_version": "0.9.0", [changed]: "mismatch" }] });
      }
      return defaultFetch(input);
    };
    await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), /previous-session-end .* did not match/);
  }
});

test("rejects a backend Jollity context mismatch and writes partial evidence", async (t) => {
  const fixture = setup(t);
  const base = setup(t);
  fixture.fetchImpl = async (input) => {
    const url = new URL(input);
    if (url.pathname.endsWith(`/projects/paritytech/200/events/${eventId}/`)) {
      return json({
        eventID: eventId,
        exception: { values: [{ value: marker }] },
        tags: [{ key: "host.session_id", value: sessionId }],
        contexts: { native_host: { hostSessionId: "623e4567-e89b-42d3-a456-426614174000" } },
        breadcrumbs: [{ category: "replay", message: "session.link", data: { data: JSON.stringify({ observation: { hostSessionId: sessionId } }) } }],
      });
    }
    return base.fetchImpl(input);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), /native host session context/);
  const partial = JSON.parse(readFileSync(fixture.evidencePath, "utf8"));
  assert.equal(partial.verified, false);
  assert.equal(partial.failure.includes(token), false);
});

test("rejects 401 and 403 without leaking the token", async (t) => {
  for (const status of [401, 403]) {
    const fixture = setup(t);
    fixture.fetchImpl = async () => json({ detail: "private" }, status);
    await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), new RegExp(String(status)));
    assert.equal(readFileSync(fixture.evidencePath, "utf8").includes(token), false);
  }
});

test("rejects missing authentication before any Sentry request", async (t) => {
  const fixture = setup(t);
  let requests = 0;
  fixture.fetchImpl = async () => {
    requests += 1;
    return json(projects);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(fixture, { token: "" })), /SENTRY_AUTH_TOKEN is required/);
  assert.equal(requests, 0);
});

test("fails closed on a Sentry query error and preserves the failed stage", async (t) => {
  const fixture = setup(t);
  fixture.fetchImpl = async (input) => {
    const url = new URL(input);
    if (url.pathname.endsWith("/projects/")) return json(projects);
    return json({ detail: "query rejected" }, 400);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), /400/);
  const partial = JSON.parse(readFileSync(fixture.evidencePath, "utf8"));
  assert.equal(partial.verified, false);
  assert.equal(partial.stages.preflight, "passed");
  assert.equal(partial.failure.includes("query rejected"), false);
});

test("retries 429, 5xx, and network failures and resolves only a matching active DSN", async (t) => {
  const dir = mkdtempSync(join(tmpdir(), "sentry-dsn-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const destination = join(dir, "dsn");
  let projectCalls = 0;
  let keyCalls = 0;
  const fetchImpl = async (input) => {
    const url = new URL(input);
    if (url.pathname.endsWith("/projects/")) {
      projectCalls += 1;
      if (projectCalls === 1) return json({}, 429, { "retry-after": "0" });
      if (projectCalls === 2) return json({}, 503);
      return json(projects);
    }
    if (url.pathname.endsWith("/keys/")) {
      keyCalls += 1;
      if (keyCalls === 1) throw new Error("transient network failure");
      return json([{ isActive: true, projectId: 100, dsn: { public: dsn } }]);
    }
    throw new Error(`unexpected fixture URL: ${url.pathname}`);
  };
  const resolved = await resolveAndroidDsn({
    apiBase: "https://de.sentry.io", org: "paritytech", nativeProject: "polkadot-android",
    jollityProject: "jollity-next", token, destination, fetchImpl, sleep: async () => {},
  });
  assert.equal(projectCalls, 3);
  assert.equal(keyCalls, 2);
  assert.equal(resolved.resolvedFromApi, true);
  assert.equal(readFileSync(destination, "utf8"), dsn);
  await assert.rejects(resolveAndroidDsn({
    apiBase: "https://de.sentry.io", org: "paritytech", nativeProject: "polkadot-android",
    jollityProject: "jollity-next", token, destination, overrideDsn: "https://wrong@o1.ingest.de.sentry.io/200",
    fetchImpl: async (input) => {
      const url = new URL(input);
      return url.pathname.endsWith("/projects/") ? json(projects) : json([{ isActive: true, projectId: 100, dsn: { public: dsn } }]);
    }, sleep: async () => {},
  }), /does not belong/);
});

test("rejects a deceptive DSN hostname even when the key belongs to the configured project", async (t) => {
  const dir = mkdtempSync(join(tmpdir(), "sentry-dsn-host-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const destination = join(dir, "dsn");
  const deceptiveDsn = "https://public-key@evil-ingest.de.sentry.io/100";
  await assert.rejects(resolveAndroidDsn({
    apiBase: "https://de.sentry.io", org: "paritytech", nativeProject: "polkadot-android",
    jollityProject: "jollity-next", token, destination,
    fetchImpl: async (input) => {
      const url = new URL(input);
      return url.pathname.endsWith("/projects/")
        ? json(projects)
        : json([{ isActive: true, projectId: 100, dsn: { public: deceptiveDsn } }]);
    },
    sleep: async () => {},
  }), /not an EU key/);
});

test("rejects native and Jollity project IDs that differ only by JSON type", async (t) => {
  const fixture = setup(t);
  fixture.fetchImpl = async (input) => {
    const url = new URL(input);
    if (url.pathname.endsWith("/projects/")) {
      return json([
        { id: "100", slug: "polkadot-android" },
        { id: 100, slug: "jollity-next" },
      ]);
    }
    throw new Error(`unexpected fixture URL: ${url.pathname}`);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), /must be distinct/);
});

test("empty rows fail after the bounded poll and preserve missing lanes", async (t) => {
  const fixture = setup(t);
  fixture.fetchImpl = async (input) => {
      const url = new URL(input);
      if (url.pathname.endsWith("/projects/")) return json(projects);
      if (url.pathname.endsWith("/events/")) return json({ data: [] });
      throw new Error(`unexpected fixture URL: ${url.pathname}`);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(fixture)), /incomplete Host\/Jollity/);
  const partial = JSON.parse(readFileSync(fixture.evidencePath, "utf8"));
  assert.deepEqual(partial.missing, ["native", "jollity", "log"]);
});

test("polling stops at the configured attempt bound", async (t) => {
  const fixture = setup(t);
  let queries = 0;
  let sleeps = 0;
  fixture.fetchImpl = async (input) => {
    const url = new URL(input);
    if (url.pathname.endsWith("/projects/")) return json(projects);
    if (url.pathname.endsWith("/events/")) {
      queries += 1;
      return json({ data: [] });
    }
    throw new Error(`unexpected fixture URL: ${url.pathname}`);
  };
  await assert.rejects(verifySentryIngestion(verifyOptions(fixture, {
    pollAttempts: 3,
    pollIntervalMs: 1,
    sleep: async () => { sleeps += 1; },
  })), /incomplete Host\/Jollity/);
  assert.equal(queries, 9);
  assert.equal(sleeps, 2);
});
