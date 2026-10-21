import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import { hasMarkerEvent, hasPreviousSessionEnd, validateTelemetryCapture } from "./telemetry-acceptance.mjs";

const id = "123e4567-e89b-42d3-a456-426614174000";
const eventId = "abcdef0123456789abcdef0123456789";
const marker = "native-telemetry-e2e-deadbeef";
const attrs = Object.fromEntries(
  Object.entries({
    "host.session_id": id,
    "host.native_app_version": "1.2.3",
    "host.app_build": "456",
    "host.app_variant": "nightly",
    "truapi.host_version": "0.9.0",
    "truapi.schema_version": "1",
  }).map(([name, value]) => [name, { type: "string", value }]),
);
const envelope = (type, payload) => `${JSON.stringify({ sent_at: "now" })}\n${JSON.stringify({ type })}\n${JSON.stringify(payload)}\n`;
const capture = {
  href: "https://app.dim2next.paseo/?token=must-not-leave-memory",
  bootstrap: {
    hostSessionId: id,
    appVersion: "1.2.3",
    appBuild: "456",
    appVariant: "nightly",
    platform: "android",
    osVersion: "16",
    truapiHostVersion: "0.9.0",
    schemaVersion: "1",
  },
  descriptor: { frozen: true, writable: false, configurable: false },
  envelopes: [
    envelope("log", { items: [{ body: "previous-session-end", attributes: attrs }] }),
    envelope("event", {
      event_id: eventId,
      exception: { values: [{ value: marker }] },
      tags: { "host.session_id": id },
      contexts: { native_host: { hostSessionId: id } },
      breadcrumbs: [{ category: "replay", message: "session.link", data: { data: JSON.stringify({ observation: { hostSessionId: id } }) } }],
    }),
  ],
};

test("accepts one UUID across bootstrap, log attributes, context and session link", () => {
  assert.equal(hasPreviousSessionEnd(capture), true);
  assert.equal(hasMarkerEvent(capture, marker), true);
  const result = validateTelemetryCapture(capture, {
    platform: "android",
    marker,
    candidateRef: "a".repeat(40),
    harnessSha: "b".repeat(40),
    product: "dim2next.paseo",
  });
  assert.equal(result.bootstrap.hostSessionId, id);
  assert.equal(result.event.sessionLinkHostSessionId, id);
  assert.equal(result.event.eventId, eventId);
  assert.equal(result.pageOrigin, "https://app.dim2next.paseo");
  assert.equal(JSON.stringify(result).includes("sent_at"), false);
  assert.equal(JSON.stringify(result).includes("must-not-leave-memory"), false);
});

test("rejects a session-link mismatch", () => {
  const broken = structuredClone(capture);
  const payload = JSON.parse(broken.envelopes[1].split("\n")[2]);
  payload.breadcrumbs[0].data.data = JSON.stringify({ observation: { hostSessionId: "623e4567-e89b-42d3-a456-426614174000" } });
  broken.envelopes[1] = envelope("event", payload);
  assert.throws(
    () => validateTelemetryCapture(broken, { platform: "android", marker, product: "dim2next.paseo" }),
    /session\.link hostSessionId/,
  );
});

test("rejects a mutable bootstrap", () => {
  const broken = structuredClone(capture);
  broken.descriptor.writable = true;
  assert.throws(
    () => validateTelemetryCapture(broken, { platform: "android", marker, product: "dim2next.paseo" }),
    /bootstrap writable/,
  );
});

test("distinguishes an evicted session link from a UUID mismatch", () => {
  const broken = structuredClone(capture);
  const payload = JSON.parse(broken.envelopes[1].split("\n")[2]);
  payload.breadcrumbs = [];
  broken.envelopes[1] = envelope("event", payload);
  assert.throws(
    () => validateTelemetryCapture(broken, { platform: "android", marker, product: "dim2next.paseo" }),
    /possibly evicted/,
  );
});

test("distinguishes a truncated session link from a UUID mismatch", () => {
  const broken = structuredClone(capture);
  const payload = JSON.parse(broken.envelopes[1].split("\n")[2]);
  payload.breadcrumbs[0].data.data = '{"observation":…[truncated]';
  broken.envelopes[1] = envelope("event", payload);
  assert.throws(
    () => validateTelemetryCapture(broken, { platform: "android", marker, product: "dim2next.paseo" }),
    /was truncated/,
  );
});

test("the document-start tap leaves relative application fetches alone and captures Sentry", async () => {
  const originalCalls = [];
  const context = {
    URL,
    Blob,
    ArrayBuffer,
    TextDecoder,
    Request,
    Response,
    location: { href: "https://app.dim2next.paseo/" },
    setTimeout: () => 0,
    fetch: async (...args) => {
      originalCalls.push(args);
      return new Response("application");
    },
  };
  context.window = context;
  context.top = context;
  context.globalThis = context;
  vm.createContext(context);
  vm.runInContext(readFileSync(new URL("./telemetry-tap.js", import.meta.url), "utf8").replace("__NATIVE_TELEMETRY_PASS_THROUGH__", "false"), context);

  assert.equal(await (await context.fetch("/health")).text(), "application");
  assert.equal(originalCalls.length, 1);
  const sentry = await context.fetch("https://o1.ingest.de.sentry.io/api/7/envelope/", { body: "envelope" });
  assert.equal(sentry.status, 200);
  assert.deepEqual(Array.from(context.__nativeTelemetryAcceptance.snapshot().envelopes), ["envelope"]);
});

test("the tap triggers the marked probe as soon as the previous-session log is captured", async () => {
  let thrown;
  const context = {
    URL,
    Blob,
    ArrayBuffer,
    TextDecoder,
    Request,
    Response,
    location: { href: "https://app.dim2next.paseo/" },
    setTimeout: (callback) => {
      try {
        callback();
      } catch (error) {
        thrown = error;
      }
      return 0;
    },
    fetch: async () => new Response("application"),
  };
  context.window = context;
  context.top = context;
  context.globalThis = context;
  vm.createContext(context);
  const source = readFileSync(new URL("./telemetry-tap.js", import.meta.url), "utf8").replace(
    '"__NATIVE_TELEMETRY_MARKER__"',
    JSON.stringify(marker),
  ).replace("__NATIVE_TELEMETRY_PASS_THROUGH__", "false");
  vm.runInContext(source, context);

  await context.fetch("https://o1.ingest.de.sentry.io/api/7/envelope/", { body: "previous-session-end" });
  assert.equal(thrown?.message, marker);
});

test("the explicit ingestion tap passes Sentry requests to the original fetch", async () => {
  let calls = 0;
  const context = {
    URL,
    Blob,
    ArrayBuffer,
    TextDecoder,
    Request,
    Response,
    location: { href: "https://app.dim2next.paseo/" },
    setTimeout: () => 0,
    fetch: async () => {
      calls += 1;
      return new Response("accepted by Sentry", { status: 202 });
    },
  };
  context.window = context;
  context.top = context;
  context.globalThis = context;
  vm.createContext(context);
  const source = readFileSync(new URL("./telemetry-tap.js", import.meta.url), "utf8")
    .replace('"__NATIVE_TELEMETRY_MARKER__"', JSON.stringify(marker))
    .replace("__NATIVE_TELEMETRY_PASS_THROUGH__", "true");
  vm.runInContext(source, context);
  const response = await context.fetch("https://o1.ingest.de.sentry.io/api/7/envelope/", { body: "event" });
  assert.equal(response.status, 202);
  assert.equal(await response.text(), "accepted by Sentry");
  assert.equal(calls, 1);
  assert.deepEqual(Array.from(context.__nativeTelemetryAcceptance.snapshot().envelopes), ["event"]);
});
