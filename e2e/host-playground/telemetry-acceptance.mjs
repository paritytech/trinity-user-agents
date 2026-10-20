const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function envelopeItems(body) {
  const lines = String(body).split("\n");
  const items = [];
  for (let index = 1; index + 1 < lines.length; index += 2) {
    try {
      items.push({ header: JSON.parse(lines[index]), payload: JSON.parse(lines[index + 1]) });
    } catch {
      // A different Sentry item may be binary or use a shape this gate does not consume.
    }
  }
  return items;
}

function attribute(record, name) {
  const value = record?.attributes?.[name];
  return value && typeof value === "object" && "value" in value ? value.value : value;
}

function tagsOf(event) {
  if (Array.isArray(event?.tags)) return Object.fromEntries(event.tags);
  return event?.tags && typeof event.tags === "object" ? event.tags : {};
}

function breadcrumbsOf(event) {
  if (Array.isArray(event?.breadcrumbs)) return event.breadcrumbs;
  return Array.isArray(event?.breadcrumbs?.values) ? event.breadcrumbs.values : [];
}

function exceptionText(event) {
  const values = event?.exception?.values;
  if (Array.isArray(values)) return values.map((value) => value?.value).filter(Boolean).join("\n");
  return typeof event?.message === "string" ? event.message : event?.message?.formatted ?? "";
}

export function hasPreviousSessionEnd(capture) {
  return capture.envelopes.some((body) =>
    envelopeItems(body).some(({ header, payload }) =>
      header.type === "log" && payload?.items?.some((record) => record.body === "previous-session-end"),
    ),
  );
}

export function hasMarkerEvent(capture, marker) {
  return capture.envelopes.some((body) =>
    envelopeItems(body).some(({ header, payload }) => header.type === "event" && exceptionText(payload).includes(marker)),
  );
}

function requireEqual(actual, expected, label) {
  if (actual !== expected) throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}

function requireString(value, label) {
  if (typeof value !== "string" || value.length === 0) throw new Error(`${label}: expected a non-empty string`);
  return value;
}

export function validateTelemetryCapture(capture, { platform, marker, candidateRef, harnessSha, product }) {
  const bootstrap = capture?.bootstrap;
  if (!bootstrap || typeof bootstrap !== "object") throw new Error("native telemetry bootstrap is absent");
  if (!UUID.test(bootstrap.hostSessionId ?? "")) throw new Error("hostSessionId is not a UUID");
  requireEqual(bootstrap.platform, platform, "bootstrap.platform");
  requireEqual(bootstrap.appVariant, "nightly", "bootstrap.appVariant");
  requireEqual(bootstrap.schemaVersion, "1", "bootstrap.schemaVersion");
  requireString(bootstrap.appVersion, "bootstrap.appVersion");
  requireString(bootstrap.appBuild, "bootstrap.appBuild");
  requireString(bootstrap.osVersion, "bootstrap.osVersion");
  requireString(bootstrap.truapiHostVersion, "bootstrap.truapiHostVersion");
  if (bootstrap.sourceCommit !== undefined) requireEqual(bootstrap.sourceCommit, candidateRef, "bootstrap.sourceCommit");
  requireEqual(capture.descriptor?.frozen, true, "bootstrap frozen");
  requireEqual(capture.descriptor?.writable, false, "bootstrap writable");
  requireEqual(capture.descriptor?.configurable, false, "bootstrap configurable");
  let pageUrl;
  try {
    pageUrl = new URL(capture.href);
  } catch {
    throw new Error("captured page URL is invalid");
  }
  if (!pageUrl.hostname.includes(product.split(".")[0])) throw new Error(`captured page ${pageUrl.hostname} is not ${product}`);

  const items = capture.envelopes.flatMap(envelopeItems);
  const previous = items
    .filter(({ header }) => header.type === "log")
    .flatMap(({ payload }) => payload?.items ?? [])
    .find((record) => record.body === "previous-session-end");
  if (!previous) throw new Error("previous-session-end Sentry log was not captured");

  const expectedAttributes = {
    "host.session_id": bootstrap.hostSessionId,
    "host.native_app_version": bootstrap.appVersion,
    "host.app_build": bootstrap.appBuild,
    "host.app_variant": bootstrap.appVariant,
    "truapi.host_version": bootstrap.truapiHostVersion,
    "truapi.schema_version": bootstrap.schemaVersion,
  };
  for (const [name, expected] of Object.entries(expectedAttributes)) {
    requireEqual(attribute(previous, name), expected, `previous-session-end ${name}`);
  }

  const event = items.find(({ header, payload }) => header.type === "event" && exceptionText(payload).includes(marker))?.payload;
  if (!event) throw new Error("marked Sentry error event was not captured");
  const tags = tagsOf(event);
  requireEqual(tags["host.session_id"], bootstrap.hostSessionId, "event host.session_id");
  requireEqual(event.contexts?.native_host?.hostSessionId, bootstrap.hostSessionId, "event native_host.hostSessionId");

  const link = breadcrumbsOf(event).find((crumb) => crumb?.category === "replay" && crumb?.message === "session.link");
  if (!link) throw new Error("session.link breadcrumb was absent from the immediate probe event (possibly evicted)");
  const linkText = link.data?.data;
  if (typeof linkText !== "string") throw new Error("session.link breadcrumb data is absent");
  if (linkText.includes("[truncated]")) throw new Error("session.link breadcrumb data was truncated before validation");
  let linkData;
  try {
    linkData = JSON.parse(linkText);
  } catch {
    throw new Error("session.link breadcrumb data is not JSON");
  }
  requireEqual(linkData?.observation?.hostSessionId, bootstrap.hostSessionId, "session.link hostSessionId");

  return {
    platform,
    candidateRef,
    harnessSha,
    product,
    pageOrigin: pageUrl.origin,
    bootstrap: {
      hostSessionId: bootstrap.hostSessionId,
      appVersion: bootstrap.appVersion,
      appBuild: bootstrap.appBuild,
      appVariant: bootstrap.appVariant,
      platform: bootstrap.platform,
      osVersion: bootstrap.osVersion,
      truapiHostVersion: bootstrap.truapiHostVersion,
      ...(bootstrap.sourceCommit ? { sourceCommit: bootstrap.sourceCommit } : {}),
      schemaVersion: bootstrap.schemaVersion,
      descriptor: capture.descriptor,
    },
    previousSessionEnd: expectedAttributes,
    event: {
      marker,
      hostSessionId: tags["host.session_id"],
      nativeHostSessionId: event.contexts.native_host.hostSessionId,
      sessionLinkHostSessionId: linkData.observation.hostSessionId,
    },
  };
}
