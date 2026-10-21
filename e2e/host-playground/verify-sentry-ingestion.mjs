import { existsSync, readFileSync, writeFileSync } from "node:fs";

const DEFAULT_API_BASE = "https://de.sentry.io";
const RETRY_LIMIT = 5;
const POLL_ATTEMPTS = 20;
const POLL_INTERVAL_MS = 15_000;

function required(value, name) {
  if (typeof value !== "string" || value.trim() === "") throw new Error(`${name} is required`);
  return value.trim();
}

function pollSetting(value, fallback, maximum, name) {
  if (value === undefined || value === "") return fallback;
  if (!/^\d+$/.test(String(value)) || Number(value) < 1 || Number(value) > maximum) {
    throw new Error(`${name} must be an integer between 1 and ${maximum}`);
  }
  return Number(value);
}

function normalizeApiBase(value) {
  const url = new URL(value || DEFAULT_API_BASE);
  if (url.protocol !== "https:" || url.hostname !== "de.sentry.io") {
    throw new Error("SENTRY_API_BASE must be an HTTPS Sentry EU API host");
  }
  return url.origin;
}

function apiUrl(base, path, params) {
  const url = new URL(`/api/0/${path.replace(/^\//, "")}`, base);
  for (const [key, value] of Object.entries(params)) {
    if (Array.isArray(value)) for (const item of value) url.searchParams.append(key, item);
    else url.searchParams.set(key, value);
  }
  return url;
}

function retryDelay(response, attempt) {
  const retryAfter = response?.headers?.get?.("retry-after");
  if (retryAfter && /^\d+(\.\d+)?$/.test(retryAfter)) {
    return Math.min(5_000, Math.max(0, Number(retryAfter) * 1_000));
  }
  return Math.min(5_000, 500 * 2 ** attempt);
}

async function requestJson(url, token, { fetchImpl, sleep, retries = RETRY_LIMIT }) {
  for (let attempt = 0; attempt <= retries; attempt++) {
    let response;
    try {
      response = await fetchImpl(url, {
        headers: { authorization: `Bearer ${token}`, accept: "application/json" },
        signal: AbortSignal.timeout(15_000),
      });
    } catch {
      if (attempt === retries) throw new Error("Sentry API network request failed after bounded retries");
      await sleep(retryDelay(null, attempt));
      continue;
    }

    if (response.status === 401) throw new Error("Sentry API rejected SENTRY_AUTH_TOKEN (401)");
    if (response.status === 403) throw new Error("Sentry API denied access; token needs org:read (403)");
    if (response.status === 429 || response.status >= 500) {
      if (attempt === retries) throw new Error(`Sentry API remained unavailable (${response.status}) after bounded retries`);
      await sleep(retryDelay(response, attempt));
      continue;
    }
    if (!response.ok) throw new Error(`Sentry API request failed (${response.status})`);
    try {
      return await response.json();
    } catch {
      throw new Error("Sentry API returned invalid JSON");
    }
  }
  throw new Error("Sentry API retry limit exhausted");
}

function rowsOf(payload) {
  if (!payload || !Array.isArray(payload.data)) throw new Error("Sentry query returned no data rows");
  return payload.data;
}

function queryUrl(base, org, projectId, dataset, sessionId) {
  return apiUrl(base, `organizations/${encodeURIComponent(org)}/events/`, {
    dataset,
    project: String(projectId),
    statsPeriod: "1h",
    per_page: "100",
    query: `host.session_id:${sessionId}`,
    field: dataset === "logs"
      ? ["id", "timestamp", "message", "host.app_build", "host.native_app_version", "truapi.host_version"]
      : ["id", "timestamp", "message"],
  });
}

function eventUrl(base, org, projectId, eventId) {
  return apiUrl(base, `projects/${encodeURIComponent(org)}/${encodeURIComponent(String(projectId))}/events/${encodeURIComponent(eventId)}/`, {});
}

function matchRows(rows, includes) {
  return rows.filter((row) =>
    typeof row.message === "string" &&
    includes.every((text) => row.message.includes(text)),
  );
}

function eventContextValue(event, key) {
  return event?.context?.[key] ?? event?.extra?.[key] ?? null;
}

function assertLogAttributes(row, bootstrap) {
  assertEqual(String(row["host.app_build"] ?? ""), String(bootstrap.appBuild), "previous-session-end host.app_build");
  assertEqual(String(row["host.native_app_version"] ?? ""), bootstrap.appVersion, "previous-session-end host.native_app_version");
  assertEqual(String(row["truapi.host_version"] ?? ""), bootstrap.truapiHostVersion, "previous-session-end truapi.host_version");
}

function sanitizedRow(row, project) {
  return { project, eventId: String(row.id), timestamp: String(row.timestamp) };
}

function tagsOf(event) {
  if (Array.isArray(event?.tags)) {
    return Object.fromEntries(event.tags.map((tag) => Array.isArray(tag) ? tag : [tag?.key, tag?.value]));
  }
  return event?.tags && typeof event.tags === "object" ? event.tags : {};
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) throw new Error(`${label} did not match the captured Host session`);
}

function breadcrumbLink(event) {
  const crumbs = Array.isArray(event?.entries)
    ? event.entries.filter((entry) => entry.type === "breadcrumbs").flatMap((entry) => entry.data?.values ?? [])
    : (Array.isArray(event?.breadcrumbs) ? event.breadcrumbs : event?.breadcrumbs?.values ?? []);
  const crumb = crumbs.find((entry) => entry?.category === "replay" && entry?.message === "session.link");
  let value = crumb?.data?.data;
  if (typeof value === "string") {
    try { value = JSON.parse(value); } catch { return null; }
  }
  return value?.observation?.hostSessionId ?? null;
}

function assertJollityEvent(event, { eventId, sessionId, marker }) {
  const returnedId = event?.eventID ?? event?.eventId ?? event?.id;
  assertEqual(String(returnedId ?? "").toLowerCase(), eventId.toLowerCase(), "Jollity event ID");
  const exceptionValues = event?.entries?.filter((entry) => entry.type === "exception")
    .flatMap((entry) => entry.data?.values ?? []) ?? event?.exception?.values ?? [];
  const marked = exceptionValues.some((value) => value?.value === marker) || event?.message === marker || event?.title === marker;
  assertEqual(marked, true, "Jollity marker");
  assertEqual(tagsOf(event)["host.session_id"], sessionId, "Jollity host.session_id tag");
  assertEqual(event?.contexts?.native_host?.hostSessionId, sessionId, "Jollity native host session context");
  assertEqual(breadcrumbLink(event), sessionId, "Jollity session.link breadcrumb");
}

function assertNativeEvent(event, { eventId, sessionId, bootstrap }) {
  const returnedId = event?.eventID ?? event?.eventId ?? event?.id;
  assertEqual(String(returnedId ?? "").toLowerCase(), eventId.toLowerCase(), "native event ID");
  assertEqual(event?.message, "host.webview.created", "native event message");
  const tags = tagsOf(event);
  assertEqual(tags["host.session_id"], sessionId, "native host.session_id tag");
  assertEqual(tags["host.app_variant"], bootstrap.appVariant, "native app variant tag");
  assertEqual(tags["host.platform"], "android", "native platform tag");
  assertEqual(tags["host.native_app_version"], bootstrap.appVersion, "native app version tag");
  assertEqual(tags["truapi.host_version"], bootstrap.truapiHostVersion, "native TruAPI host version tag");
  const appBuild = eventContextValue(event, "host.app_build");
  if (typeof appBuild !== "number" || !Number.isFinite(appBuild)) {
    throw new Error("native host.app_build extra is not a finite number");
  }
  assertEqual(String(appBuild), String(bootstrap.appBuild), "native host.app_build extra");
  const monotonicMs = eventContextValue(event, "event.monotonic_ms");
  if (typeof monotonicMs !== "number" || !Number.isFinite(monotonicMs)) {
    throw new Error("native event.monotonic_ms extra is not a finite number");
  }
}

export async function verifySentryIngestion({
  apiBase,
  org,
  nativeProject,
  jollityProject,
  token,
  sessionId,
  marker,
  jollityEventId,
  bootstrap,
  evidencePath,
  candidateRef,
  harnessSha,
  fetchImpl = fetch,
  sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  pollAttempts = POLL_ATTEMPTS,
  pollIntervalMs = POLL_INTERVAL_MS,
}) {
  const base = normalizeApiBase(apiBase || DEFAULT_API_BASE);
  const organization = required(org, "SENTRY_ORG");
  const native = required(nativeProject, "SENTRY_NATIVE_PROJECT");
  const jollity = required(jollityProject, "SENTRY_JOLLITY_PROJECT");
  const auth = required(token, "SENTRY_AUTH_TOKEN");
  const id = required(sessionId, "host session UUID");
  const probe = required(marker, "Jollity event marker");
  const output = required(evidencePath, "TELEMETRY_EVIDENCE");
  const evidence = {
    verified: false,
    apiRegion: "eu",
    organization,
    candidateRef,
    harnessSha,
    hostSessionId: id,
    marker: probe,
    stages: { preflight: "pending", native: "pending", jollity: "pending", log: "pending" },
  };
  const save = () => writeFileSync(output, `${JSON.stringify(evidence, null, 2)}\n`, { mode: 0o600 });
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(id)) {
    throw new Error("host session ID is not a UUID");
  }
  if (!/^[0-9a-f]{40}$/.test(candidateRef || "") || !/^[0-9a-f]{40}$/.test(harnessSha || "")) {
    throw new Error("candidate and harness revisions must be full commit IDs");
  }

  if (!/^[0-9a-f]{32}$/i.test(jollityEventId || "")) throw new Error("captured Jollity event ID is invalid");
  if (!bootstrap || bootstrap.hostSessionId !== id) throw new Error("captured Jollity bootstrap is invalid");

  try {
    const projects = await requestJson(
      apiUrl(base, `organizations/${encodeURIComponent(organization)}/projects/`, {}),
      auth,
      { fetchImpl, sleep },
    );
    if (!Array.isArray(projects)) throw new Error("Sentry organization preflight returned no project list");
    const nativeInfo = projects.find((project) => project?.slug === native);
    const jollityInfo = projects.find((project) => project?.slug === jollity);
    if (!nativeInfo || !/^\d+$/.test(String(nativeInfo.id))) throw new Error("SENTRY_NATIVE_PROJECT has no numeric project ID");
    if (!jollityInfo || !/^\d+$/.test(String(jollityInfo.id))) throw new Error("SENTRY_JOLLITY_PROJECT has no numeric project ID");
    if (nativeInfo.id === jollityInfo.id) throw new Error("native and Jollity Sentry projects must be distinct");
    evidence.projects = { native: { slug: native, id: String(nativeInfo.id) }, jollity: { slug: jollity, id: String(jollityInfo.id) } };
    evidence.stages.preflight = "passed";
    save();

    let matched;
    let lastCounts = {};
    for (let attempt = 0; attempt < pollAttempts; attempt++) {
      const [nativePayload, jollityPayload, logsPayload] = await Promise.all([
        requestJson(queryUrl(base, organization, nativeInfo.id, "errors", id), auth, { fetchImpl, sleep }),
        requestJson(queryUrl(base, organization, jollityInfo.id, "errors", id), auth, { fetchImpl, sleep }),
        requestJson(queryUrl(base, organization, jollityInfo.id, "logs", id), auth, { fetchImpl, sleep }),
      ]);
      const nativeRows = rowsOf(nativePayload);
      const jollityRows = rowsOf(jollityPayload);
      const logRows = rowsOf(logsPayload);
      const nativeEvent = matchRows(nativeRows, ["host.webview.created"]);
      const jollityEvent = matchRows(jollityRows, [probe]);
      const previousSessionEnd = matchRows(logRows, []).filter((row) => row.message === "previous-session-end");
      lastCounts = { nativeRows: nativeRows.length, jollityRows: jollityRows.length, logRows: logRows.length };
      evidence.queryAttempts = attempt + 1;
      evidence.lastRowCounts = lastCounts;
      if (nativeEvent.length) evidence.stages.native = "rows-found";
      if (jollityEvent.length) evidence.stages.jollity = "rows-found";
      if (previousSessionEnd.length) evidence.stages.log = "rows-found";
      if (nativeEvent.length && jollityEvent.length && previousSessionEnd.length) {
        matched = { nativeEvent, jollityEvent, previousSessionEnd };
        break;
      }
      save();
      if (attempt + 1 < pollAttempts) await sleep(pollIntervalMs);
    }
    if (!matched) {
      evidence.failure = "Sentry ingestion query returned incomplete Host/Jollity session evidence";
      evidence.missing = Object.entries(evidence.stages).filter(([, status]) => status === "pending").map(([name]) => name);
      save();
      throw new Error(evidence.failure);
    }

    const nativeRow = matched.nativeEvent.find((row) => row.id);
    const nativeDetail = await requestJson(eventUrl(base, organization, nativeInfo.id, String(nativeRow.id)), auth, { fetchImpl, sleep });
    assertNativeEvent(nativeDetail, { eventId: String(nativeRow.id), sessionId: id, bootstrap });
    evidence.stages.native = "passed";

    const jollityRow = matched.jollityEvent.find((row) => String(row.id).toLowerCase() === jollityEventId.toLowerCase());
    if (!jollityRow) throw new Error("captured marked Jollity event ID was not present in Sentry query rows");
    const jollityDetail = await requestJson(eventUrl(base, organization, jollityInfo.id, jollityEventId), auth, { fetchImpl, sleep });
    assertJollityEvent(jollityDetail, { eventId: jollityEventId, sessionId: id, marker: probe });
    evidence.stages.jollity = "passed";
    assertLogAttributes(matched.previousSessionEnd[0], bootstrap);
    evidence.stages.log = "passed";
    evidence.verified = true;
    evidence.projects.native.event = sanitizedRow(nativeRow, native);
    evidence.projects.jollity.markedEvent = sanitizedRow(jollityRow, jollity);
    evidence.projects.jollity.previousSessionEnd = sanitizedRow(matched.previousSessionEnd[0], jollity);
    evidence.checkedAt = new Date().toISOString();
    save();
    return evidence;
  } catch (error) {
    evidence.failure = error instanceof Error ? error.message : "Sentry verification failed";
    save();
    throw error;
  }
}

export async function resolveAndroidDsn({
  apiBase,
  org,
  nativeProject,
  jollityProject,
  token,
  overrideDsn,
  destination,
  fetchImpl = fetch,
  sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
}) {
  const base = normalizeApiBase(apiBase || DEFAULT_API_BASE);
  const organization = required(org, "SENTRY_ORG");
  const native = required(nativeProject, "SENTRY_NATIVE_PROJECT");
  const jollity = required(jollityProject, "SENTRY_JOLLITY_PROJECT");
  const auth = required(token, "SENTRY_AUTH_TOKEN");
  const output = required(destination, "SENTRY_DSN_FILE");
  const projects = await requestJson(
    apiUrl(base, `organizations/${encodeURIComponent(organization)}/projects/`, {}),
    auth,
    { fetchImpl, sleep },
  );
  if (!Array.isArray(projects)) throw new Error("Sentry organization preflight returned no project list");
  const project = projects.find((entry) => entry?.slug === native);
  if (!project || project.id === undefined) throw new Error("SENTRY_NATIVE_PROJECT is not accessible in SENTRY_ORG");
  if (native === jollity || !projects.some((entry) => entry?.slug === jollity)) {
    throw new Error("SENTRY_JOLLITY_PROJECT is not accessible or is not distinct in SENTRY_ORG");
  }
  const keys = await requestJson(
    apiUrl(base, `projects/${encodeURIComponent(organization)}/${encodeURIComponent(native)}/keys/`, { status: "active" }),
    auth,
    { fetchImpl, sleep },
  );
  if (!Array.isArray(keys)) throw new Error("Sentry native project preflight returned no client keys");
  const matching = keys.filter((key) =>
    key?.isActive === true && String(key.projectId) === String(project.id) &&
    typeof key?.dsn?.public === "string" && key.dsn.public.length > 0,
  );
  if (!matching.length) throw new Error("SENTRY_NATIVE_PROJECT has no active public client DSN");
  const publicDsns = new Set(matching.map((key) => key.dsn.public));
  if (overrideDsn && !publicDsns.has(overrideDsn)) {
    throw new Error("SENTRY_DSN_ANDROID does not belong to SENTRY_NATIVE_PROJECT");
  }
  const dsn = overrideDsn || matching[0].dsn.public;
  let parsed;
  try {
    parsed = new URL(dsn);
  } catch {
    throw new Error("SENTRY_NATIVE_PROJECT returned an invalid public DSN");
  }
  if (parsed.protocol !== "https:" || !parsed.hostname.endsWith("ingest.de.sentry.io") || parsed.pathname !== `/${project.id}`) {
    throw new Error("SENTRY_NATIVE_PROJECT public DSN is not an EU key for the configured project");
  }
  writeFileSync(output, dsn, { mode: 0o600 });
  return { project: native, resolvedFromApi: !overrideDsn };
}

async function main() {
  const deviceEvidencePath = required(process.env.TELEMETRY_DEVICE_EVIDENCE, "TELEMETRY_DEVICE_EVIDENCE");
  const evidencePath = required(process.env.TELEMETRY_EVIDENCE, "TELEMETRY_EVIDENCE");
  try {
    let telemetry;
    try {
      telemetry = JSON.parse(readFileSync(deviceEvidencePath, "utf8"));
    } catch {
      throw new Error("sanitized device telemetry evidence is missing or invalid");
    }
    if (telemetry?.event?.hostSessionId !== telemetry?.bootstrap?.hostSessionId) {
      throw new Error("device telemetry evidence has inconsistent session IDs");
    }
    const evidence = await verifySentryIngestion({
      apiBase: process.env.SENTRY_API_BASE,
      org: process.env.SENTRY_ORG,
      nativeProject: process.env.SENTRY_NATIVE_PROJECT,
      jollityProject: process.env.SENTRY_JOLLITY_PROJECT,
      token: process.env.SENTRY_AUTH_TOKEN,
      sessionId: telemetry.bootstrap.hostSessionId,
      marker: telemetry.event.marker,
      jollityEventId: telemetry.event.eventId,
      bootstrap: telemetry.bootstrap,
      evidencePath,
      candidateRef: process.env.E2E_CANDIDATE_REF,
      harnessSha: process.env.E2E_HARNESS_SHA,
      pollAttempts: pollSetting(process.env.SENTRY_POLL_ATTEMPTS, POLL_ATTEMPTS, 60, "SENTRY_POLL_ATTEMPTS"),
      pollIntervalMs: pollSetting(process.env.SENTRY_POLL_INTERVAL_MS, POLL_INTERVAL_MS, 60_000, "SENTRY_POLL_INTERVAL_MS"),
    });
    console.log(`Verified Host and Jollity Sentry ingestion for session ${evidence.hostSessionId}`);
  } catch (error) {
    if (!existsSync(evidencePath)) {
      writeFileSync(evidencePath, `${JSON.stringify({ verified: false, apiRegion: "eu", failure: error.message, stages: { preflight: "not-completed", native: "not-completed", jollity: "not-completed", log: "not-completed" } }, null, 2)}\n`, { mode: 0o600 });
    }
    throw error;
  }
}

if (process.argv[1] && new URL(import.meta.url).pathname === process.argv[1]) {
  main().catch((error) => {
    console.error(`[sentry acceptance] ${error.message}`);
    process.exitCode = 1;
  });
}
