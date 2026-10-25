// The dashboard server: one process holding the GitHub token, the relay token
// and the webhook secrets, so no viewer and no workflow needs them. It runs
// as a product on Parity's Kubernetes clusters; see ci-dashboard/README.md.
//
//   bun ci-dashboard/server/main.ts
//
// Configuration comes from the environment; see ci-dashboard/README.md.

import { mkdir } from "node:fs/promises";
import { join } from "node:path";

import config from "../src/config";
import { collect, githubGet, type Snapshot } from "../src/data";
import { createApp, MAX_BODY_BYTES } from "./app";
import { post, queue, type Relay } from "./matrix";
import { Monitor } from "./monitor";

const env = (name: string, fallback?: string): string => {
  const value = process.env[name] ?? fallback;
  if (value === undefined) {
    throw new Error(`${name} is not set`);
  }
  return value;
};

const port = Number(env("PORT", "8787"));
const dashboardUrl = env("DASHBOARD_URL", `http://localhost:${port}`);
const stateDir = env("STATE_DIR", join(import.meta.dir, "..", "state"));
const siteDir = env("SITE_DIR", join(import.meta.dir, "..", "dist"));
const githubToken = env("GITHUB_TOKEN", "");
const relay: Relay = {
  url: env("NOTIFICATION_BOT_URL", ""),
  token: env("NOTIFICATION_BOT_TOKEN", ""),
  rooms: env("MATRIX_ROOM_IDS", "")
    .split(",")
    .map((room) => room.trim())
    .filter(Boolean),
};

await mkdir(stateDir, { recursive: true });

// Staging has no relay token, so it logs what production would post.
const notify = queue(async (message) => {
  if (!relay.url || !relay.token || relay.rooms.length === 0) {
    console.log(`(Matrix not set up) ${message.text}`);
    return;
  }
  await post(relay, message);
});

// --- GitHub data -------------------------------------------------------------

const get = githubGet(config.repository, githubToken || undefined);
let snapshot: Snapshot | undefined;
let loading: Promise<void> | undefined;

function refresh() {
  loading ??= collect(get, config)
    .then((fresh) => {
      snapshot = fresh;
    })
    .catch((error) => console.error("GitHub data:", error))
    .finally(() => {
      loading = undefined;
    });
}

// --- Checks -------------------------------------------------------------------

const monitor = new Monitor(config, stateDir, notify, dashboardUrl);
await monitor.load();

const every = (ms: number, task: () => Promise<unknown> | void) => {
  const run = () =>
    Promise.resolve(task()).catch((error) => console.error(error));
  void run();
  setInterval(run, ms);
};

const MINUTE = 60_000;
every(MINUTE, () => monitor.checkEndpoints());
every(60 * MINUTE, () => monitor.checkCertificates());
// Webhooks keep GitHub data current; this catches anything they miss. A
// refresh takes about a dozen requests, so without a token it runs seldom
// enough to stay under the anonymous 60 an hour.
every(githubToken ? 5 * MINUTE : 30 * MINUTE, refresh);

// The smoke checks run once a day, after the nightlies.
const SMOKE_AT_UTC = { hour: 6, minute: 37 };
function scheduleSmoke() {
  const next = new Date();
  next.setUTCHours(SMOKE_AT_UTC.hour, SMOKE_AT_UTC.minute, 0, 0);
  if (next.getTime() <= Date.now()) next.setUTCDate(next.getUTCDate() + 1);
  setTimeout(() => {
    void monitor
      .runSmoke(join(import.meta.dir, "..", "smoke.sh"))
      .catch((error) => console.error("Smoke checks:", error))
      .finally(scheduleSmoke);
  }, next.getTime() - Date.now());
}
scheduleSmoke();

// --- HTTP ---------------------------------------------------------------------

const server = Bun.serve({
  port,
  maxRequestBodySize: MAX_BODY_BYTES,
  fetch: createApp({
    config,
    siteDir,
    snapshot: () => snapshot,
    refresh,
    status: () => monitor.status,
    notify,
    releaseExists: async (tag) =>
      (await get(`/releases/tags/${encodeURIComponent(tag)}`)) !== undefined,
    tagGraceMs: MINUTE,
    githubWebhookSecret: env("GITHUB_WEBHOOK_SECRET", ""),
    sentryClientSecret: env("SENTRY_CLIENT_SECRET", ""),
  }),
});

console.log(`CI dashboard on http://localhost:${server.port}`);
