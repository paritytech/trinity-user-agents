import type { Nightly } from "./config";

/** The parts of a GitHub release this dashboard reads. */
export type Release = {
  tag_name: string;
  name: string | null;
  html_url: string;
  published_at: string | null;
  body: string | null;
  draft: boolean;
  prerelease: boolean;
  assets: { name: string; size: number; browser_download_url: string }[];
};

export type NightlyBuild = {
  label: string;
  tag: string;
  url: string;
  publishedAt: string;
  /** The versionCode the nightly workflow writes into the notes. */
  build?: number;
  /**
   * The nightly run the notes link in their host-playground e2e block. The
   * e2e is a job in it, not a run of its own.
   */
  nightlyRunId?: number;
  assets: { name: string; size: number; url: string }[];
};

/**
 * The run id in the host-playground e2e block the nightly workflow writes
 * into the release notes, between `<!-- host-playground-e2e -->` markers.
 * It is the nightly's own run, which called the e2e.
 */
export function nightlyRunId(body: string | null): number | undefined {
  const block =
    /<!-- host-playground-e2e -->([\s\S]*?)<!-- \/host-playground-e2e -->/.exec(
      body ?? "",
    )?.[1];
  const id = block && /\/actions\/runs\/(\d+)/.exec(block)?.[1];
  return id ? Number(id) : undefined;
}

/**
 * Every dated nightly, newest first. Pointer releases such as
 * `nightly-android` do not match a `tagPrefix`, so they are skipped.
 */
export function nightlyBuilds(
  releases: Release[],
  nightlies: Nightly[],
): NightlyBuild[] {
  return releases
    .flatMap((release) => {
      const nightly = nightlies.find((n) =>
        release.tag_name.startsWith(n.tagPrefix),
      );
      if (!nightly || release.draft || !release.published_at) {
        return [];
      }
      const build = /Nightly build `(\d+)`/.exec(release.body ?? "")?.[1];
      return [
        {
          label: nightly.label,
          tag: release.tag_name,
          url: release.html_url,
          publishedAt: release.published_at,
          build: build ? Number(build) : undefined,
          nightlyRunId: nightlyRunId(release.body),
          assets: release.assets
            .filter((a) => nightly.assets.includes(a.name))
            .map((a) => ({
              name: a.name,
              size: a.size,
              url: a.browser_download_url,
            })),
        },
      ];
    })
    .sort((a, b) => b.publishedAt.localeCompare(a.publishedAt));
}

/**
 * The newest published release whose tag starts with `prefix`, with the
 * version that follows it: `@parity/truapi@0.26.0` gives `0.26.0`.
 */
export function latestTagged(
  releases: Release[],
  prefix: string,
): { version: string; url: string; publishedAt: string } | undefined {
  const newest = releases
    .filter((r) => !r.draft && r.published_at && r.tag_name.startsWith(prefix))
    .sort((a, b) => b.published_at!.localeCompare(a.published_at!))[0];
  return newest
    ? {
        version: newest.tag_name.slice(prefix.length),
        url: newest.html_url,
        publishedAt: newest.published_at!,
      }
    : undefined;
}

export type SdkRelease = {
  name: string;
  version: string;
  url: string;
  publishedAt: string;
};

/**
 * Releases that are not prereleases, newest first: the npm packages, the
 * iOS host and the stack, tagged `<package>@<version>`. Pointer releases
 * without a version are left out.
 */
export function sdkReleases(releases: Release[]): SdkRelease[] {
  return releases
    .flatMap((release) => {
      const at = release.tag_name.lastIndexOf("@");
      if (
        release.draft ||
        release.prerelease ||
        !release.published_at ||
        at <= 0
      ) {
        return [];
      }
      return [
        {
          name: release.tag_name.slice(0, at),
          version: release.tag_name.slice(at + 1),
          url: release.html_url,
          publishedAt: release.published_at,
        },
      ];
    })
    .sort((a, b) => b.publishedAt.localeCompare(a.publishedAt));
}

/** The newest nightly of each kind, in the order of `nightlies`. */
export function latestNightlies(
  releases: Release[],
  nightlies: Nightly[],
): (NightlyBuild | undefined)[] {
  const builds = nightlyBuilds(releases, nightlies);
  return nightlies.map((n) => builds.find((b) => b.label === n.label));
}

/** The parts of a workflow run this dashboard reads. */
export type WorkflowRun = {
  html_url: string;
  status: string;
  conclusion: string | null;
  event: string;
  head_branch: string | null;
  created_at: string;
  display_title?: string;
};

/** The parts of a job this dashboard reads. */
export type Job = {
  name: string;
  html_url: string;
  status: string;
  conclusion: string | null;
};

/**
 * Some of a run's jobs read as one run: failing if one failed, running while
 * one runs, passing once they passed. Undefined when none ran, so the caller
 * can look at an earlier run.
 */
export function jobsRun(
  run: WorkflowRun,
  jobs: Job[],
): WorkflowRun | undefined {
  const ran = jobs.filter((job) => job.conclusion !== "skipped");
  if (ran.length === 0) {
    return undefined;
  }
  const failed = ran.find(
    (job) => job.conclusion === "failure" || job.conclusion === "timed_out",
  );
  const running = ran.find((job) => job.status !== "completed");
  const shown = failed ?? running ?? ran[0]!;
  return {
    ...run,
    html_url: shown.html_url,
    status: shown.status,
    conclusion: shown.conclusion,
  };
}

export type RunState = "passing" | "failing" | "running" | "unknown";

export function runState(run: WorkflowRun | undefined): RunState {
  if (!run) {
    return "unknown";
  }
  if (run.status !== "completed") {
    return "running";
  }
  // Skipped and cancelled runs say nothing about whether the workflow works.
  if (run.conclusion === "success") {
    return "passing";
  }
  if (run.conclusion === "failure" || run.conclusion === "timed_out") {
    return "failing";
  }
  return "unknown";
}
