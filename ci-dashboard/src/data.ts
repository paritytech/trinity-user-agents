import type { Config, WatchedWorkflow } from "./config";
import {
  jobsRun,
  latestTagged,
  nightlyBuilds,
  runState,
  sdkReleases,
  type NightlyBuild,
  type Release,
  type Job,
  type SdkRelease,
  type WorkflowRun,
} from "./github";

/**
 * Everything the page shows from GitHub. The server reads it with its own
 * token, so a viewer makes no GitHub API calls.
 */
export type Snapshot = {
  generatedAt: string;
  builds: NightlyBuild[];
  releases: SdkRelease[];
  cli: { version: string; url: string; publishedAt: string } | undefined;
  /** In the order of `workflows` in config.json. */
  runs: (WorkflowRun | undefined)[];
  /** The e2e jobs that tested the newest Android nightly, read as one run. */
  e2e: WorkflowRun | undefined;
};

/**
 * Reads a path under `/repos/<repository>` of the GitHub API, or undefined
 * when it does not exist: a workflow not yet on the default branch has none.
 */
export type Get = <T>(path: string) => Promise<T | undefined>;

// Only what the page reads, so the snapshot stays small.
function slim(run: WorkflowRun | undefined): WorkflowRun | undefined {
  return (
    run && {
      html_url: run.html_url,
      status: run.status,
      conclusion: run.conclusion,
      event: run.event,
      head_branch: run.head_branch,
      created_at: run.created_at,
    }
  );
}

// How many runs back a leg is looked for, when the newest nights skipped it.
const JOB_RUNS = 5;

async function jobs(get: Get, runId: number): Promise<Job[]> {
  const page = await get<{ jobs: Job[] }>(
    `/actions/runs/${runId}/jobs?per_page=100`,
  );
  return page?.jobs ?? [];
}

// Skipped and cancelled runs are passed over: Release, for one, skips every
// commit that is not a release.
async function latestRun(
  get: Get,
  workflow: WatchedWorkflow,
): Promise<WorkflowRun | undefined> {
  const page = await get<{ workflow_runs: (WorkflowRun & { id: number })[] }>(
    `/actions/workflows/${workflow.file}/runs?per_page=20&exclude_pull_requests=true`,
  );
  const runs = (page?.workflow_runs ?? []).filter(
    (run) => !workflow.title || run.display_title === workflow.title,
  );
  if (workflow.job) {
    const prefix = workflow.job;
    for (const run of runs.slice(0, JOB_RUNS)) {
      const leg = jobsRun(
        run,
        (await jobs(get, run.id)).filter((job) => job.name.startsWith(prefix)),
      );
      if (leg && runState(leg) !== "unknown") return slim(leg);
    }
    return undefined;
  }
  return slim(runs.find((run) => runState(run) !== "unknown") ?? runs[0]);
}

// Nightlies are most of the releases, so one page holds only a few weeks of
// package releases. Three pages go back months.
const RELEASE_PAGES = 3;

async function releases(get: Get): Promise<Release[]> {
  const all: Release[] = [];
  for (let page = 1; page <= RELEASE_PAGES; page++) {
    const batch =
      (await get<Release[]>(`/releases?per_page=100&page=${page}`)) ?? [];
    all.push(...batch);
    if (batch.length < 100) break;
  }
  return all;
}

export async function collect(
  get: Get,
  config: Config,
  now = new Date(),
): Promise<Snapshot> {
  const [all, runs] = await Promise.all([
    releases(get),
    Promise.all(config.workflows.map((w) => latestRun(get, w))),
  ]);
  const builds = nightlyBuilds(all, config.nightlies);
  const android = config.nightlies.find((n) => n.label === "Android");
  const runId = builds.find((b) => b.label === "Android")?.nightlyRunId;
  let e2e: WorkflowRun | undefined;
  if (runId && android?.e2eJob) {
    const [run, legs] = await Promise.all([
      get<WorkflowRun>(`/actions/runs/${runId}`),
      jobs(get, runId),
    ]);
    const prefix = android.e2eJob;
    e2e =
      run &&
      slim(
        jobsRun(
          run,
          legs.filter((job) => job.name.startsWith(prefix)),
        ),
      );
  }
  return {
    generatedAt: now.toISOString(),
    builds,
    releases: sdkReleases(all),
    cli: latestTagged(all, config.downloads.cliReleasePrefix),
    runs,
    e2e,
  };
}

/** A GitHub API reader, with a token when one is given. */
export function githubGet(repository: string, token?: string): Get {
  return async <T>(path: string) => {
    const response = await fetch(
      `https://api.github.com/repos/${repository}${path}`,
      {
        headers: {
          accept: "application/vnd.github+json",
          "x-github-api-version": "2022-11-28",
          ...(token ? { authorization: `Bearer ${token}` } : {}),
        },
        signal: AbortSignal.timeout(30_000),
      },
    );
    if (response.status === 404) {
      return undefined;
    }
    if (!response.ok) {
      throw new Error(`GitHub API ${response.status} for ${path}`);
    }
    return (await response.json()) as T;
  };
}
