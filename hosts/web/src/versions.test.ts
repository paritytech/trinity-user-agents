import { describe, expect, test } from "bun:test";
import {
  versionSections,
  type BuildInfo,
  type RuntimeInfo,
} from "./versions.js";

const BUILD: BuildInfo = {
  packages: {
    truapi: "1.2.3",
    truapiHost: "1.2.4",
    truapiProvider: "0.5.0",
    truapiDebugger: "0.1.0",
  },
  core: { version: "1.1.0", sha256: "a".repeat(64) },
  verifiableSha256: "b".repeat(64),
  providerSha256: "c".repeat(64),
  source: { commit: "abc1234", dirty: false },
};

const RUNTIME: RuntimeInfo = {
  clientSchema: "s".repeat(64),
  clientCodec: 7,
  coreSchema: "s".repeat(64),
};

const sections = (build = BUILD, runtime = RUNTIME) =>
  versionSections(build, runtime);

const technical = (label: string, build = BUILD, runtime = RUNTIME) => {
  const found = sections(build, runtime).technical.find(
    (r) => r.label === label,
  );
  if (!found) throw new Error(`no technical row ${label}`);
  return found;
};

describe("summary", () => {
  test("lists the everyday versions and nothing technical", () => {
    expect(sections().summary).toEqual([
      { label: "Client", value: "1.2.3" },
      { label: "Host", value: "1.2.4" },
      { label: "Core bundle", value: "1.1.0" },
      { label: "Provider", value: "0.5.0" },
      { label: "Debugger", value: "0.1.0" },
    ]);
  });

  // The core bundle is a separate artifact that can differ from the package
  // beside it, so the two only share a line when they are the same string.
  test("keeps host and core apart when their versions differ", () => {
    const labels = sections().summary.map((row) => row.label);
    expect(labels).toContain("Host");
    expect(labels).toContain("Core bundle");
  });

  test("shares one line only when host and core are the same version", () => {
    const same: BuildInfo = {
      ...BUILD,
      core: { ...BUILD.core, version: "1.2.4" },
    };
    expect(
      sections(same).summary.filter((row) => row.label.includes("core")),
    ).toEqual([{ label: "Host and core", value: "1.2.4" }]);
  });

  test("does not merge an unknown core version into the host's", () => {
    const unknown: BuildInfo = {
      ...BUILD,
      core: { version: null, sha256: null },
    };
    expect(sections(unknown).summary).toContainEqual({
      label: "Core bundle",
      value: "unknown",
      state: "unknown",
    });
  });

  test("says a package version is unknown instead of guessing it", () => {
    const build: BuildInfo = {
      ...BUILD,
      packages: { ...BUILD.packages, truapiProvider: null },
    };
    expect(sections(build).summary).toContainEqual({
      label: "Provider",
      value: "unknown",
      state: "unknown",
    });
  });
});

describe("mismatch warning", () => {
  test("is absent when the core and client wire schemas match", () => {
    expect(sections().mismatch).toBeNull();
  });

  // A core and a client on different wire tables decode frames wrongly, so the
  // difference has to be a visible warning, not a different-looking hash.
  test("names both schemas when they differ", () => {
    const { mismatch, technical: rows } = sections(BUILD, {
      ...RUNTIME,
      coreSchema: "z".repeat(64),
    });
    expect(mismatch).toContain("zzzzzzzz");
    expect(mismatch).toContain("ssssssss");
    expect(rows.find((r) => r.label === "Wire schema (core)")?.state).toBe(
      "warning",
    );
  });

  // Not reporting a schema is missing metadata, not a mismatch.
  test("is not raised while the core has not reported a schema", () => {
    expect(
      sections(BUILD, { ...RUNTIME, coreSchema: undefined }).mismatch,
    ).toBeNull();
  });
});

describe("technical details", () => {
  test("holds the file hashes with their full value behind the title", () => {
    expect(technical("Core bundle hash")).toEqual({
      label: "Core bundle hash",
      value: "aaaaaaaa",
      detail: undefined,
      title: `sha256 ${"a".repeat(64)}`,
    });
    expect(technical("Provider hash").value).toBe("cccccccc");
    expect(technical("Ring-VRF module hash").detail).toBe(
      "no version of its own",
    );
  });

  test("reports a matching core wire schema without a warning state", () => {
    const schema = technical("Wire schema (core)");
    expect(schema.detail).toBe("matches client");
    expect(schema.state).toBeUndefined();
  });

  test("says so while the core has not reported a wire schema", () => {
    expect(
      technical("Wire schema (core)", BUILD, {
        ...RUNTIME,
        coreSchema: undefined,
      }),
    ).toEqual({
      label: "Wire schema (core)",
      value: "not reported",
      state: "unknown",
    });
  });

  test("does not invent a core codec version", () => {
    expect(technical("Wire codec")).toEqual({
      label: "Wire codec",
      value: "client 7",
      detail: "core does not report one",
    });
  });

  test("marks hashes and the source as unknown when unreadable", () => {
    const build: BuildInfo = {
      ...BUILD,
      core: { version: "1.1.0", sha256: null },
      verifiableSha256: null,
      providerSha256: null,
      source: { commit: null, dirty: false },
    };
    for (const label of [
      "Core bundle hash",
      "Provider hash",
      "Ring-VRF module hash",
      "Source",
    ])
      expect(technical(label, build).state).toBe("unknown");
  });

  test("flags a source tree with uncommitted changes", () => {
    expect(
      technical("Source", {
        ...BUILD,
        source: { commit: "abc1234", dirty: true },
      }),
    ).toEqual({
      label: "Source",
      value: "abc1234",
      detail: "uncommitted changes",
    });
  });
});
