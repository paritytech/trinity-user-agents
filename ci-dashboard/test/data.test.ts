import { describe, expect, test } from "bun:test";
import { readdir } from "node:fs/promises";
import { join } from "node:path";

import config from "../src/config";
import { collect, type Get } from "../src/data";

const run = (conclusion: string | null, status = "completed") => ({
  html_url: "https://github.test/runs/1",
  status,
  conclusion,
  event: "schedule",
  head_branch: "main",
  created_at: "2026-10-25T06:00:00Z",
  // Fields the snapshot drops.
  id: 1,
  head_sha: "abc",
});

function fakeGet(responses: Record<string, unknown>): Get {
  return (async (path: string) => {
    const key =
      Object.keys(responses).find((k) => k === path) ??
      Object.keys(responses)
        .filter((k) => path.startsWith(k))
        .sort((a, b) => b.length - a.length)[0];
    return key ? responses[key] : undefined;
  }) as Get;
}

const job = (
  name: string,
  conclusion: string | null,
  status = "completed",
) => ({
  name,
  html_url: `https://github.test/job/${encodeURIComponent(name)}`,
  status,
  conclusion,
});

const releases = [
  {
    tag_name: "android-20261026-2246-cc8aa21",
    name: "Nightly",
    html_url: "https://github.test/r/1",
    published_at: "2026-10-26T22:46:00Z",
    body: "Nightly build `1049`\n<!-- host-playground-e2e -->[Run](https://github.com/o/r/actions/runs/77)<!-- /host-playground-e2e -->",
    draft: false,
    prerelease: true,
    assets: [],
  },
  {
    tag_name: "@parity/truapi@0.26.0",
    name: "@parity/truapi 0.26.0",
    html_url: "https://github.test/r/2",
    published_at: "2026-10-21T20:31:42Z",
    body: "",
    draft: false,
    prerelease: false,
    assets: [],
  },
];

describe("collect", () => {
  test("gathers builds, releases, runs and the nightly's e2e jobs", async () => {
    const snapshot = await collect(
      fakeGet({
        "/releases?per_page=100&page=1": releases,
        "/actions/workflows/android-nightly.yml": {
          workflow_runs: [run("skipped"), run("success")],
        },
        // The nightly run passed; its e2e job, which never fails it, did not.
        "/actions/runs/77/jobs": {
          jobs: [
            job("nightly", "success"),
            job("host-playground e2e / iOS simulator", "skipped"),
            job("host-playground e2e / Android emulator", "failure"),
          ],
        },
        "/actions/runs/77": run("success"),
      }),
      config,
      new Date("2026-10-27T00:00:00Z"),
    );

    expect(snapshot.generatedAt).toBe("2026-10-27T00:00:00.000Z");
    expect(snapshot.builds[0]).toMatchObject({ build: 1049, nightlyRunId: 77 });
    expect(snapshot.releases.map((r) => r.version)).toEqual(["0.26.0"]);
    expect(snapshot.cli?.version).toBe("0.26.0");
    // Skipped runs are passed over, and extra fields are dropped.
    expect(snapshot.runs[0]).toEqual({
      html_url: "https://github.test/runs/1",
      status: "completed",
      conclusion: "success",
      event: "schedule",
      head_branch: "main",
      created_at: "2026-10-25T06:00:00Z",
    });
    // A workflow with no runs yet reads as undefined, not an error.
    expect(snapshot.runs[1]).toBeUndefined();
    expect(snapshot.e2e?.conclusion).toBe("failure");
    expect(snapshot.e2e?.html_url).toContain("Android%20emulator");
  });

  test("reads further pages of releases, which are mostly nightlies", async () => {
    const nightlies = Array.from({ length: 100 }, (_, i) => ({
      ...releases[0]!,
      tag_name: `android-${i}`,
    }));
    const snapshot = await collect(
      fakeGet({
        "/releases?per_page=100&page=1": nightlies,
        "/releases?per_page=100&page=2": [releases[1]],
      }),
      config,
    );
    expect(snapshot.cli?.version).toBe("0.26.0");
  });

  test("reads the main iOS night and its simulator leg", async () => {
    const ios = (id: number, title: string, conclusion: string) => ({
      ...run(conclusion),
      id,
      display_title: title,
      html_url: `https://github.test/runs/${id}`,
    });
    const snapshot = await collect(
      fakeGet({
        "/actions/workflows/ios-nightly-prepare.yml": {
          workflow_runs: [
            ios(3, "iOS Nightly (release/1.2)", "failure"),
            ios(2, "iOS Nightly (main)", "success"),
            ios(1, "iOS Nightly (main)", "failure"),
          ],
        },
        // Nothing changed on night 2, so its legs were skipped.
        "/actions/runs/2/jobs": {
          jobs: [
            job(
              "nightly / Simulator bundle / Build & publish simulator .app",
              "skipped",
            ),
          ],
        },
        "/actions/runs/1/jobs": {
          jobs: [
            job("nightly / TestFlight / Check build result", "success"),
            job(
              "nightly / Simulator bundle / Build & publish simulator .app",
              "failure",
            ),
          ],
        },
      }),
      config,
    );
    const at = (label: string) =>
      snapshot.runs[config.workflows.findIndex((w) => w.label === label)];
    // The branch night is left out.
    expect(at("iOS nightly")?.html_url).toBe("https://github.test/runs/2");
    // The leg comes from the last night that ran it.
    expect(at("iOS simulator nightly")).toMatchObject({
      conclusion: "failure",
      html_url: expect.stringContaining("Simulator%20bundle"),
    });
  });
});

describe("config.json", () => {
  test("names only workflows that exist", async () => {
    const dir = join(import.meta.dir, "..", "..", ".github", "workflows");
    const files = new Set(await readdir(dir));
    for (const file of [
      ...config.workflows.map((w) => w.file),
      ...config.alertOnFailure,
    ]) {
      expect({ file, exists: files.has(file) }).toEqual({ file, exists: true });
    }
  });
});
