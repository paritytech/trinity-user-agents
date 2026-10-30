// HTTP routes: the page and its data, the GitHub and Sentry webhooks, and a
// health check.

import { join, normalize } from "node:path";

import type { Config } from "../src/config";
import type { Snapshot } from "../src/data";
import type { Message } from "./matrix";
import type { Status } from "./monitor";
import {
  githubEvent,
  sentryEvent,
  verifyGithub,
  verifySentry,
} from "./webhooks";

export type Deps = {
  config: Config;
  /** Built page: index.html, dashboard.js, style.css. */
  siteDir: string;
  snapshot: () => Snapshot | undefined;
  /** Asks for fresh GitHub data, for an event that changed it. */
  refresh: () => void;
  status: () => Status;
  notify: (message: Message) => Promise<void>;
  githubWebhookSecret: string;
  sentryClientSecret: string;
};

/**
 * Bun reads a whole request body before the signature can be checked, so
 * bodies are capped well below its 128 MB default. Webhook payloads are a
 * few KB.
 */
export const MAX_BODY_BYTES = 1024 * 1024;

const json = (body: unknown, status = 200) =>
  Response.json(body, {
    status,
    headers: { "cache-control": "no-store" },
  });

// GitHub gives up on a delivery after 10 seconds, and a relay post can take
// longer, so the answer goes back first and the post follows.
function inBackground(deps: Deps, message: Message | undefined) {
  if (message) {
    deps.notify(message).catch((error) => console.error("Notify:", error));
  }
}

function parse(body: string): { ok: true; value: unknown } | { ok: false } {
  try {
    return { ok: true, value: JSON.parse(body) };
  } catch {
    return { ok: false };
  }
}

export function createApp(deps: Deps) {
  return async (request: Request): Promise<Response> => {
    const url = new URL(request.url);
    const { pathname } = url;

    if (pathname === "/up") {
      return new Response("ok");
    }

    if (pathname === "/api/dashboard" && request.method === "GET") {
      return json({ github: deps.snapshot() ?? null, checks: deps.status() });
    }

    if (pathname === "/webhooks/github" && request.method === "POST") {
      const body = await request.text();
      if (
        !verifyGithub(
          deps.githubWebhookSecret,
          body,
          request.headers.get("x-hub-signature-256"),
        )
      ) {
        return json({ message: "invalid signature" }, 401);
      }
      const payload = parse(body);
      if (!payload.ok) {
        return json({ message: "invalid JSON" }, 400);
      }
      const outcome = githubEvent(
        request.headers.get("x-github-event"),
        payload.value,
        deps.config,
      );
      if (outcome.refresh) deps.refresh();
      inBackground(deps, outcome.message);
      return json({ ok: true });
    }

    if (pathname === "/webhooks/sentry" && request.method === "POST") {
      const body = await request.text();
      if (
        !verifySentry(
          deps.sentryClientSecret,
          body,
          request.headers.get("sentry-hook-signature"),
        )
      ) {
        return json({ message: "invalid signature" }, 401);
      }
      const payload = parse(body);
      if (!payload.ok) {
        return json({ message: "invalid JSON" }, 400);
      }
      inBackground(
        deps,
        sentryEvent(
          request.headers.get("sentry-hook-resource"),
          payload.value,
          deps.config,
        ),
      );
      return json({ ok: true });
    }

    if (request.method === "GET") {
      return serveSite(deps.siteDir, pathname);
    }

    return new Response("Not found", { status: 404 });
  };
}

export async function serveSite(
  siteDir: string,
  pathname: string,
): Promise<Response> {
  const relative = pathname === "/" ? "index.html" : pathname.slice(1);
  // Refuse anything that would leave the site directory.
  const path = normalize(join(siteDir, relative));
  if (!path.startsWith(normalize(siteDir))) {
    return new Response("Not found", { status: 404 });
  }
  const file = Bun.file(path);
  if (!(await file.exists())) {
    return new Response("Not found", { status: 404 });
  }
  return new Response(file);
}
