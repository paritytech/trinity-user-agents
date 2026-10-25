import { describe, expect, test } from "bun:test";

import {
  nightlyRunId,
  latestNightlies,
  latestTagged,
  nightlyBuilds,
  runState,
  failingSigningRun,
  sdkReleases,
  type Release,
} from "../src/github";

function release(tag: string, publishedAt: string, body = ""): Release {
  return {
    tag_name: tag,
    name: tag,
    html_url: `https://github.test/releases/${tag}`,
    published_at: publishedAt,
    body,
    draft: false,
    prerelease: true,
    assets: [
      {
        name: "app-gp-nightly.apk",
        size: 10,
        browser_download_url: `https://dl.test/${tag}/gp.apk`,
      },
      {
        name: "unrelated.txt",
        size: 1,
        browser_download_url: "https://dl.test/x",
      },
    ],
  };
}

const android = {
  label: "Android",
  tagPrefix: "android-",
  assets: ["app-gp-nightly.apk"],
};

describe("latestNightlies", () => {
  test("picks the newest dated nightly and reads its build number", () => {
    const [build] = latestNightlies(
      [
        release(
          "android-20261021-2251-ee3cbb0",
          "2026-10-21T22:51:32Z",
          "Nightly build `1043`",
        ),
        release(
          "nightly-android",
          "2026-10-23T22:53:00Z",
          "Nightly build `1046`",
        ),
        release(
          "android-20261022-2240-667b633",
          "2026-10-22T22:40:54Z",
          "Nightly build `1044`",
        ),
        release("@parity/truapi@0.26.0", "2026-10-24T00:00:00Z"),
      ],
      [android],
    );
    expect(build).toMatchObject({
      tag: "android-20261022-2240-667b633",
      build: 1044,
      assets: [
        {
          name: "app-gp-nightly.apk",
          size: 10,
          url: "https://dl.test/android-20261022-2240-667b633/gp.apk",
        },
      ],
    });
  });

  test("is undefined when no release matches", () => {
    expect(latestNightlies([], [android])).toEqual([undefined]);
  });
});

describe("nightlyBuilds", () => {
  test("lists every nightly kind newest first, skipping drafts", () => {
    const simulator = {
      label: "iOS simulator",
      tagPrefix: "simulator-",
      assets: [],
    };
    const builds = nightlyBuilds(
      [
        release("android-20261021-2251-ee3cbb0", "2026-10-21T22:51:32Z"),
        release("simulator-20261022-0005-dd78483", "2026-10-22T00:05:39Z"),
        {
          ...release("android-20261023-2253-aaaaaaa", "2026-10-23T22:53:00Z"),
          draft: true,
        },
      ],
      [android, simulator],
    );
    expect(builds.map((b) => [b.label, b.tag])).toEqual([
      ["iOS simulator", "simulator-20261022-0005-dd78483"],
      ["Android", "android-20261021-2251-ee3cbb0"],
    ]);
  });
});

describe("runState", () => {
  const run = {
    html_url: "https://github.test/runs/1",
    event: "schedule",
    head_branch: "main",
    created_at: "2026-10-26T22:14:35Z",
  };

  test("maps conclusions", () => {
    expect(
      runState({ ...run, status: "completed", conclusion: "success" }),
    ).toBe("passing");
    expect(
      runState({ ...run, status: "completed", conclusion: "failure" }),
    ).toBe("failing");
    expect(
      runState({ ...run, status: "completed", conclusion: "cancelled" }),
    ).toBe("unknown");
    expect(runState({ ...run, status: "in_progress", conclusion: null })).toBe(
      "running",
    );
    expect(runState(undefined)).toBe("unknown");
  });
});

describe("latestTagged", () => {
  test("picks the newest release with the prefix and reads its version", () => {
    expect(
      latestTagged(
        [
          release("@parity/truapi-host@0.25.0", "2026-10-10T00:00:00Z"),
          release("@parity/truapi-host@0.26.0", "2026-10-21T20:31:42Z"),
          release("@parity/truapi@0.27.0", "2026-10-22T00:00:00Z"),
        ],
        "@parity/truapi-host@",
      ),
    ).toEqual({
      version: "0.26.0",
      url: "https://github.test/releases/@parity/truapi-host@0.26.0",
      publishedAt: "2026-10-21T20:31:42Z",
    });
  });

  test("is undefined when nothing matches", () => {
    expect(latestTagged([], "@parity/truapi-host@")).toBeUndefined();
  });
});

describe("nightlyRunId", () => {
  test("reads the run from the nightly's e2e block", () => {
    const body =
      "notes\n<!-- host-playground-e2e -->\n### host-playground e2e\nTested `cc8aa21`. [Run](https://github.com/o/r/actions/runs/40410898727).\n<!-- /host-playground-e2e -->";
    expect(nightlyRunId(body)).toBe(40410898727);
  });

  test("ignores runs linked outside the block", () => {
    expect(
      nightlyRunId("[CI](https://github.com/o/r/actions/runs/1)"),
    ).toBeUndefined();
    expect(nightlyRunId(null)).toBeUndefined();
  });
});

describe("sdkReleases", () => {
  test("lists versioned releases that are not prereleases, newest first", () => {
    const releases = [
      {
        ...release("@parity/truapi@0.25.0", "2026-10-10T00:00:00Z"),
        prerelease: false,
      },
      {
        ...release("@parity/truapi@0.26.0", "2026-10-21T00:00:00Z"),
        prerelease: false,
      },
      {
        ...release("android-20261022-2240-667b633", "2026-10-22T00:00:00Z"),
        prerelease: true,
      },
      {
        ...release("truapi-host-cli-stable", "2026-10-23T00:00:00Z"),
        prerelease: false,
      },
    ];
    expect(sdkReleases(releases).map((r) => [r.name, r.version])).toEqual([
      ["@parity/truapi", "0.26.0"],
      ["@parity/truapi", "0.25.0"],
    ]);
  });
});

describe("failingSigningRun", () => {
  const workflows = [
    { file: "android-nightly.yml" },
    { file: "validate-signing-credentials.yml" },
  ];
  const run = {
    html_url: "https://run",
    status: "completed",
    conclusion: "failure",
    event: "schedule",
    head_branch: "main",
    created_at: "2026-10-25T06:00:00Z",
  };

  test("returns the signing check's run while it fails", () => {
    expect(failingSigningRun(workflows, [undefined, run])).toBe(run);
  });

  test("ignores other workflows and a passing check", () => {
    expect(failingSigningRun(workflows, [run, undefined])).toBe(undefined);
    expect(
      failingSigningRun(workflows, [run, { ...run, conclusion: "success" }]),
    ).toBe(undefined);
  });
});
