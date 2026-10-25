import config from "../config.json" with { type: "json" };

export type Nightly = {
  label: string;
  tagPrefix: string;
  assets: string[];
  /**
   * The e2e job in the nightly run the release notes link. The e2e is a
   * called workflow that never fails the night, so the run's own result
   * says nothing about it.
   */
  e2eJob?: string;
};

export type WatchedWorkflow = {
  file: string;
  label: string;
  /** Only runs with this title, such as the main night of a workflow branch nights share. */
  title?: string;
  /**
   * Only the jobs whose names start with this, for a leg that runs inside
   * another workflow's run rather than in runs of its own.
   */
  job?: string;
};

export type RpcEndpoint = {
  name: string;
  url: string;
  network: string;
  minPeers: number;
};

export type Link = {
  name: string;
  url: string;
  method: "GET" | "HEAD";
  lookFor?: string;
};

export type Downloads = {
  /** What Obtainium should match among a nightly's assets. */
  obtainiumApkFilter: string;
  /** Shell lines that install the simulator build, from its release notes. */
  simulatorInstall: string[];
  /** Tag prefix of the releases that carry the CLI binaries. */
  cliReleasePrefix: string;
  cliInstall: string;
};

export type SentryProject = { slug: string; label: string };

export type Sentry = {
  /** Organisation slug. Empty until it is set up; the check then skips. */
  organization: string;
  api: string;
  projects: SentryProject[];
};

export type Smoke = {
  /** Installed from npm and imported, each daily. */
  npmPackages: string[];
  /** The APK behind the fixed nightly link, read for its package and build. */
  nightlyApk: { url: string; package: string; tagPrefix: string };
};

export type Announcements = {
  /**
   * Tags pushed without a release that are not announced, besides those
   * without a version; a trailing `*` matches any rest.
   */
  skipTags: string[];
};

export type Config = {
  repository: string;
  nightlies: Nightly[];
  downloads: Downloads;
  sentry: Sentry;
  smoke: Smoke;
  workflows: WatchedWorkflow[];
  /**
   * Workflows whose failures are posted to Matrix besides `workflows`. The
   * page leaves Release out because it skips every commit that is not a release.
   */
  alertOnFailure: string[];
  rpcEndpoints: RpcEndpoint[];
  links: Link[];
  announcements: Announcements;
};

export default config as Config;
