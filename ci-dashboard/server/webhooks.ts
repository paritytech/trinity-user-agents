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

type CreateEvent = { ref: string; ref_type: string };

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
  /**
   * A pushed tag, posted only if no release appears for it. Release creates
   * its tags with the release, so most tags are announced as releases.
   */
  tag?: { name: string; message: Message };
  /** Whether the dashboard's GitHub data is now out of date. */
  refresh: boolean;
};

/**
 * The version a release-like tag carries: `0.26.0`, `v1.2.3` or
 * `@parity/truapi@0.19.0`. Undefined for anything else, such as a backup or
 * a pointer tag.
 */
export function tagVersion(tag: string): string | undefined {
  return /(?:^|@)(v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)$/.exec(tag)?.[1];
}

/** A semver pre-release such as `0.4.0-rc.1` or `1.0.0-SNAPSHOT`. */
export function isPrerelease(version: string): boolean {
  return /^v?\d+\.\d+\.\d+-/.test(version);
}

/**
 * Whether a pushed tag is one to announce: it carries a version, is not a
 * pre-release, and is not a nightly or pointer tag.
 */
export function isAnnouncedTag(tag: string, skipTags: string[]): boolean {
  const version = tagVersion(tag);
  return (
    version !== undefined &&
    !isPrerelease(version) &&
    !skipTags.some((pattern) =>
      pattern.endsWith("*")
        ? tag.startsWith(pattern.slice(0, -1))
        : tag === pattern,
    )
  );
}

/**
 * A failed run of a watched workflow, a published release that is not a
 * prerelease (nightlies announce themselves), a versioned tag pushed without
 * a release, or a package version published to GitHub Packages, such as the
 * Android AAR. Webhooks also arrive for what workflows publish with their own
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
  if (event === "create") {
    const { ref, ref_type } = payload as CreateEvent;
    if (
      ref_type !== "tag" ||
      !isAnnouncedTag(ref, config.announcements.skipTags)
    ) {
      return { refresh: false };
    }
    const url = `https://github.com/${config.repository}/tree/${encodeURI(ref)}`;
    return {
      refresh: false,
      tag: {
        name: ref,
        message: {
          text: `🏷️ ${ref} tagged, with no release ${url}`,
          html: `🏷️ <b>${escape(ref)}</b> tagged, with no release, <a href="${escape(url)}">view tag</a>`,
        },
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
