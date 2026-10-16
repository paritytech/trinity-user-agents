#!/usr/bin/env node
// Turns a run's results.json into report.md and prints its one-line summary.
//
//   node e2e/host-playground/report.mjs <results.json>
//
// Unsupported, permission-denied and precondition-missing describe the host, not a breakage,
// and a known failure only counts as one while its message is the listed one.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

const NOT_A_FAILURE = new Set(["unsupported", "permission-denied", "precondition-missing"]);
const KNOWN_FAILURES = JSON.parse(readFileSync(new URL("./tests.json", import.meta.url), "utf8")).knownFailures ?? {};

const oneLine = (text) => String(text ?? "").replace(/\s+/g, " ").trim();
const cell = (text) => oneLine(text).replace(/\|/g, "\\|").slice(0, 200);
const failureText = (result) => oneLine(result.message) || result.status;

export function classify(result) {
  if (result.status === "success") return "passed";
  if (result.status === "skipped") return "skipped";
  if (result.status === "error" && NOT_A_FAILURE.has(result.outcome)) return result.outcome;
  const known = Object.hasOwn(KNOWN_FAILURES, result.id) ? KNOWN_FAILURES[result.id] : undefined;
  if (result.status === "error" && known && result.message === known.message) return "known-failure";
  return "failed";
}

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

/** The failed tests with their messages, then the known failures with their reasons. */
export function attentionSections(run, heading = "###") {
  const failed = run.results.filter((result) => classify(result) === "failed");
  const known = run.results.filter((result) => classify(result) === "known-failure");
  const sections = [];
  if (failed.length) {
    sections.push(
      `${heading} Failed (${failed.length})`,
      "",
      ...failed.map((result) => `- \`${result.id}\`: ${failureText(result)}`),
      "",
    );
  }
  if (known.length) {
    sections.push(
      `${heading} Known failures (${known.length})`,
      "",
      ...known.map((result) => `- \`${result.id}\`: ${KNOWN_FAILURES[result.id].reason}`),
      "",
    );
  }
  return sections;
}

function markdown(run) {
  const rows = run.results.map(
    (r) => `| \`${r.id}\` | ${classify(r)} | ${r.outcome ?? ""} | ${Math.round((r.durationMs ?? 0) / 100) / 10}s | ${cell(r.message)} |`,
  );
  return [
    `## host-playground on ${run.platform}`,
    "",
    summaryLine(run),
    "",
    `App: ${run.app ?? "unknown"}. host-playground: \`${run.product}\` as deployed, test list from \`${run.hostPlaygroundCommit?.slice(0, 9)}\`. Started ${run.startedAt}.`,
    "",
    ...attentionSections(run),
    `<details><summary>All ${run.results.length} tests</summary>`,
    "",
    "| Test | Result | Outcome | Time | Message |",
    "| --- | --- | --- | --- | --- |",
    ...rows,
    "",
    "</details>",
    "",
  ].join("\n");
}

/** One workflow error annotation per failed test, so failures show on the checks page. */
function annotations(run) {
  const escapeData = (text) => text.replace(/%/g, "%25");
  return run.results
    .filter((result) => classify(result) === "failed")
    .map((result) => `::error title=host-playground ${run.platform}::${escapeData(`${result.id}: ${failureText(result)}`)}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const path = process.argv[2];
  if (!path) {
    console.error("usage: report.mjs <results.json>");
    process.exit(2);
  }
  const run = JSON.parse(readFileSync(path, "utf8"));
  writeFileSync(join(dirname(path), "report.md"), markdown(run));
  if (process.env.GITHUB_ACTIONS === "true") for (const line of annotations(run)) console.log(line);
  console.log(summaryLine(run));
}
