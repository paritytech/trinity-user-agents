#!/usr/bin/env node
// Turns a run's results.json into report.md and prints its one-line summary.
//
//   node e2e/host-playground/report.mjs <results.json>
//
// A test passes when its log entry ends in `success`. An error that the
// playground marks as unsupported, permission-denied or precondition-missing
// is counted apart from a failure, because it says what the host offers
// rather than that something broke. A skip is a button that stayed disabled,
// which is how the playground marks a test that only runs from a worker. A
// failure `tests.json` lists under `knownFailures` is counted apart too, since
// its cause lies outside the hosts.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

const NOT_A_FAILURE = new Set(["unsupported", "permission-denied", "precondition-missing"]);
const KNOWN_FAILURES = JSON.parse(readFileSync(new URL("./tests.json", import.meta.url), "utf8")).knownFailures ?? {};

/** The bucket a single result falls in. */
export function classify(result) {
  if (result.status === "success") return "passed";
  if (result.status === "skipped") return "skipped";
  if (result.status === "error" && NOT_A_FAILURE.has(result.outcome)) return result.outcome;
  if (Object.hasOwn(KNOWN_FAILURES, result.id)) return "known-failure";
  return "failed";
}

/** One line for an announcement: the pass count, then the names of the failures. */
export function summaryLine(run) {
  const counts = {};
  const failed = [];
  for (const result of run.results) {
    const bucket = classify(result);
    counts[bucket] = (counts[bucket] ?? 0) + 1;
    if (bucket === "failed") failed.push(result.id);
  }
  const ran = run.results.length - (counts.skipped ?? 0);
  const others = Object.entries(counts)
    .filter(([bucket]) => bucket !== "passed" && bucket !== "failed")
    .map(([bucket, count]) => `${count} ${bucket}`);
  let line = `E2E (${run.platform}): ${counts.passed ?? 0}/${ran} passed`;
  if (others.length) line += `, ${others.join(", ")}`;
  if (run.fatal) line += `. Stopped early: ${run.fatal}`;
  if (failed.length) {
    const shown = failed.slice(0, 10);
    line += `. Failed: ${shown.join(", ")}${failed.length > shown.length ? ` and ${failed.length - shown.length} more` : ""}`;
  }
  return `${line}.`;
}

/** The markdown report: the summary, the build it ran on, then one row per test. */
export function markdown(run) {
  const escape = (text) => String(text ?? "").replace(/\|/g, "\\|").replace(/\s+/g, " ").slice(0, 200);
  const rows = run.results.map(
    (r) => `| \`${r.id}\` | ${classify(r)} | ${r.outcome ?? ""} | ${Math.round((r.durationMs ?? 0) / 100) / 10}s | ${escape(r.message)} |`,
  );
  return [
    `## host-playground on ${run.platform}`,
    "",
    summaryLine(run),
    "",
    `App: ${run.app ?? "unknown"}. host-playground: \`${run.product}\` as deployed, test list from \`${run.hostPlaygroundCommit?.slice(0, 9)}\`. Started ${run.startedAt}.`,
    "",
    "| Test | Result | Outcome | Time | Message |",
    "| --- | --- | --- | --- | --- |",
    ...rows,
    "",
  ].join("\n");
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const path = process.argv[2];
  if (!path) {
    console.error("usage: report.mjs <results.json>");
    process.exit(2);
  }
  const run = JSON.parse(readFileSync(path, "utf8"));
  writeFileSync(join(dirname(path), "report.md"), markdown(run));
  console.log(summaryLine(run));
}
