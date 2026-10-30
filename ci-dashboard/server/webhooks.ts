// GitHub and Sentry webhooks: verify the signature, then turn the events the
// dashboard cares about into Matrix messages.

import { createHmac, timingSafeEqual } from "node:crypto";

import type { Config } from "../src/config";
import { escape, type Message } from "./matrix";

/** HMAC-SHA256 of the raw body, compared in constant time. */
export function verify(
  secret: string,
  body: string,
  signature: string | null,
  prefix = "",
): boolean {
  if (!secret || !signature?.startsWith(prefix)) {
    return false;
  }
  const expected = Buffer.from(
    createHmac("sha256", secret).update(body).digest("hex"),
  );
  const given = Buffer.from(signature.slice(prefix.length));
  return expected.length === given.length && timingSafeEqual(expected, given);
}

/** GitHub sends `X-Hub-Signature-256: sha256=<hex>`. */
export const verifyGithub = (
  secret: string,
  body: string,
  header: string | null,
) => verify(secret, body, header, "sha256=");

/** Sentry sends `Sentry-Hook-Signature: <hex>`. */
export const verifySentry = (
  secret: string,
  body: string,
  header: string | null,
) => verify(secret, body, header);

type WorkflowRunEvent = {
  action: string;
  workflow_run: {
    name: string;
    display_title: string;
    path: string;
    conclusion: string | null;
    event: string;
    head_branch: string | null;
    html_url: string;
  };
};

type ReleaseEvent = {
  action: string;
  release: {
    name: string | null;
    tag_name: string;
    html_url: string;
    prerelease: boolean;
    draft: boolean;
  };
};

type PackageEvent = {
  action: string;
  package: {
    name: string;
    package_type: string;
    html_url: string;
    package_version: { version: string } | null;
  };
};

export type GithubOutcome = {
  /** Something to post, if the event is one the room should hear about. */
  message?: Message;
  /** Whether the dashboard's GitHub data is now out of date. */
  refresh: boolean;
};

/**
 * A failed run of a watched workflow, a published release that is not a
 * prerelease (nightlies announce themselves), or a package version published
 * to GitHub Packages, such as the Android AAR. Webhooks also arrive for what workflows publish with their own
 * token, which never triggers other workflows.
 */
export function githubEvent(
  event: string | null,
  payload: unknown,
  config: Config,
): GithubOutcome {
  if (event === "workflow_run") {
    const { action, workflow_run: run } = payload as WorkflowRunEvent;
    if (action !== "completed") {
      return { refresh: false };
    }
    const file = run.path.split("/").pop() ?? "";
    // A title picks the main night out of the branch nights that share a workflow.
    const watched =
      config.workflows.some(
        (w) => w.file === file && (!w.title || w.title === run.display_title),
      ) || config.alertOnFailure.includes(file);
    const failed =
      run.conclusion === "failure" || run.conclusion === "timed_out";
    // Pull request runs are the author's to follow, not the room's.
    if (!watched || !failed || run.event === "pull_request") {
      return { refresh: watched };
    }
    const branch = run.head_branch ?? "?";
    return {
      refresh: true,
      message: {
        text: `❌ ${run.name} failed\nBranch: ${branch} (${run.event})\nRun: ${run.html_url}`,
        html: `❌ <b>${escape(run.name)} failed</b><br>Branch: <code>${escape(branch)}</code> (${escape(run.event)})<br><a href="${escape(run.html_url)}">View run</a>`,
      },
    };
  }
  if (event === "release") {
    const { action, release } = payload as ReleaseEvent;
    if (action !== "published" || release.draft) {
      return { refresh: false };
    }
    if (release.prerelease) {
      return { refresh: true };
    }
    const name = release.name || release.tag_name;
    return {
      refresh: true,
      message: {
        text: `📦 ${name} released ${release.html_url}`,
        html: `📦 <b>${escape(name)}</b> released, <a href="${escape(release.html_url)}">release notes</a>`,
      },
    };
  }
  if (event === "package") {
    const { action, package: pkg } = payload as PackageEvent;
    const version = pkg?.package_version?.version ?? "";
    // Snapshots and pre-releases are left out, like prerelease releases.
    if (action !== "published" || !/^v?\d+\.\d+\.\d+$/.test(version)) {
      return { refresh: false };
    }
    return {
      refresh: false,
      message: {
        text: `📦 ${pkg.name} ${version} published to GitHub Packages (${pkg.package_type}) ${pkg.html_url}`,
        html: `📦 <b>${escape(pkg.name)} ${escape(version)}</b> published to GitHub Packages (${escape(pkg.package_type)}), <a href="${escape(pkg.html_url)}">package</a>`,
      },
    };
  }
  return { refresh: false };
}

type SentryIssueEvent = {
  action: string;
  data: {
    issue: {
      shortId: string;
      title: string;
      level: string;
      web_url?: string;
      permalink?: string;
      project: { slug: string };
    };
  };
};

/** A new issue in one of the Sentry projects in config.json. */
export function sentryEvent(
  resource: string | null,
  payload: unknown,
  config: Config,
): Message | undefined {
  if (resource !== "issue") {
    return undefined;
  }
  const { action, data } = payload as SentryIssueEvent;
  const project = config.sentry.projects.find(
    (p) => p.slug === data?.issue?.project?.slug,
  );
  if (action !== "created" || !project) {
    return undefined;
  }
  const { shortId, title, level } = data.issue;
  const url = data.issue.web_url ?? data.issue.permalink ?? "";
  return {
    text: `🐞 ${project.label}: ${shortId} ${title} (${level}) ${url}`,
    html: `🐞 ${escape(project.label)}: <a href="${escape(url)}">${escape(shortId)}</a> ${escape(title)} (${escape(level)})`,
  };
}
