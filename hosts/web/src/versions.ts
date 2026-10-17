/**
 * Facts about what this host was built from, measured when the dev server
 * starts or the build runs. `null` means the fact could not be read.
 */
export interface BuildInfo {
  /** `version` from each linked package's `package.json`. */
  packages: {
    truapi: string | null;
    truapiHost: string | null;
    truapiProvider: string | null;
    truapiDebugger: string | null;
  };
  /**
   * The WASM core this host loads, the `testing` bundle. `version` is the
   * bundle's own `package.json`, written when that bundle was built, which is
   * not necessarily the version of the packages beside it. `sha256` is of the
   * `.wasm` file served.
   */
  core: { version: string | null; sha256: string | null };
  /** SHA-256 of the ring-VRF module served beside the core. */
  verifiableSha256: string | null;
  /** SHA-256 of the light-client provider `.wasm` served. */
  providerSha256: string | null;
  /** The source tree's commit, and whether it has uncommitted changes. */
  source: { commit: string | null; dirty: boolean };
}

/** What the running page and core report about themselves. */
export interface RuntimeInfo {
  /** The wire-schema hash compiled into the page's `@parity/truapi`. */
  clientSchema: string;
  /** The wire codec version compiled into the page's `@parity/truapi`. */
  clientCodec: number;
  /** The wire-schema hash the loaded core reports; undefined until it does. */
  coreSchema: string | undefined;
}

/** One line of the versions list. */
export interface VersionRow {
  label: string;
  value: string;
  /** Extra, secondary text on the same line. */
  detail?: string;
  /** The full value behind an abbreviated one. */
  title?: string;
  /** The exact value a click on the value copies, when it differs from what is shown. */
  copy?: string;
  /** The exact value a click on the detail copies. */
  copyDetail?: string;
  state?: "unknown" | "warning";
}

/** What the menu shows: everyday versions, a warning if needed, and the rest. */
export interface VersionSections {
  /** The versions people read every day. */
  summary: VersionRow[];
  /** Set only when the core and the client really are on different wire schemas. */
  mismatch: string | null;
  /** Hashes, wire details and the source revision, shown on request. */
  technical: VersionRow[];
}

const SHORT = 8;

/** The first characters of a hash: enough to compare, the full one is the title. */
function short(hash: string): string {
  return hash.slice(0, SHORT);
}

function fact(value: string | null): Pick<VersionRow, "value" | "state"> {
  return value === null ? { value: "unknown", state: "unknown" } : { value };
}

function hashRow(
  label: string,
  sha256: string | null,
  detail?: string,
): VersionRow {
  if (sha256 === null) return { label, value: "unknown", state: "unknown" };
  return {
    label,
    value: short(sha256),
    detail,
    title: `sha256 ${sha256}`,
  };
}

/**
 * The host package and the core bundle are two facts. The bundle is a separate
 * artifact that can be older or newer than the package beside it, so they
 * share a line only when both are known and the same string.
 */
function hostAndCore(build: BuildInfo): VersionRow[] {
  const host = build.packages.truapiHost;
  const core = build.core.version;
  if (host !== null && host === core)
    return [{ label: "Host and core", value: host }];
  return [
    { label: "Host", ...fact(host) },
    { label: "Core bundle", ...fact(core) },
  ];
}

/**
 * The lines the menu shows.
 *
 * A package version is a manifest string. The core bundle's own version and
 * the hash of the file served say what is loaded. The wire lines come from the
 * running core. Nothing here is inferred: a fact that cannot be read is
 * `unknown`.
 */
export function versionSections(
  build: BuildInfo,
  runtime: RuntimeInfo,
): VersionSections {
  const { packages } = build;
  const mismatch =
    runtime.coreSchema !== undefined &&
    runtime.coreSchema !== runtime.clientSchema;

  const schemaRow: VersionRow =
    runtime.coreSchema === undefined
      ? { label: "Wire schema (core)", value: "not reported", state: "unknown" }
      : {
          label: "Wire schema (core)",
          value: short(runtime.coreSchema),
          detail: mismatch
            ? `client ${short(runtime.clientSchema)}`
            : "matches client",
          title: `core ${runtime.coreSchema}\nclient ${runtime.clientSchema}`,
          state: mismatch ? "warning" : undefined,
        };

  return {
    summary: [
      { label: "Client", ...fact(packages.truapi) },
      ...hostAndCore(build),
      { label: "Provider", ...fact(packages.truapiProvider) },
      { label: "Debugger", ...fact(packages.truapiDebugger) },
    ],
    mismatch: mismatch
      ? `The core's wire schema (${short(runtime.coreSchema!)}) differs from the client's (${short(runtime.clientSchema)}). Frames may decode wrongly.`
      : null,
    technical: [
      hashRow("Core bundle hash", build.core.sha256),
      schemaRow,
      {
        label: "Wire codec",
        value: `client ${runtime.clientCodec}`,
        detail: "core does not report one",
      },
      hashRow("Provider hash", build.providerSha256),
      hashRow(
        "Ring-VRF module hash",
        build.verifiableSha256,
        "no version of its own",
      ),
      {
        label: "Source",
        ...fact(build.source.commit),
        detail: build.source.dirty ? "uncommitted changes" : undefined,
      },
    ],
  };
}
