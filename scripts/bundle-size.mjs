#!/usr/bin/env node
/**
 * Measure the JS and WASM files of the given asset groups (raw, gzip, brotli)
 * and print a markdown report, compared with a baseline snapshot when the
 * given one exists. `--assets` takes the list the `bundle-size` action does,
 * one `<name> <dir> [!<prefix>...]` group per line, and every group must be
 * built first.
 *
 *   node scripts/bundle-size.mjs --assets <list> [--baseline <snapshot.json>]
 *                                [--snapshot <out.json>]
 *
 * `COMMIT_SHA` names the measured commit, and `GITHUB_SERVER_URL` plus
 * `GITHUB_REPOSITORY` turn commits into links.
 */

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  measureGroups,
  parseAssets,
  renderReport,
} from "./lib/bundle-size.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const { values } = parseArgs({
  options: {
    assets: { type: "string" },
    baseline: { type: "string" },
    snapshot: { type: "string" },
  },
});

if (!values.assets) throw new Error("--assets is required");

const current = {
  commit: process.env.COMMIT_SHA || undefined,
  files: measureGroups(root, parseAssets(values.assets)),
};
if (values.snapshot) {
  writeFileSync(values.snapshot, `${JSON.stringify(current, null, 2)}\n`);
}

const baseline =
  values.baseline && existsSync(values.baseline)
    ? JSON.parse(readFileSync(values.baseline, "utf8"))
    : undefined;
const { GITHUB_SERVER_URL: server, GITHUB_REPOSITORY: repo } = process.env;
process.stdout.write(
  renderReport({
    current,
    baseline,
    repoUrl: server && repo ? `${server}/${repo}` : undefined,
  }),
);
