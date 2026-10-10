#!/usr/bin/env node
// Prints a nightly prerelease's notes with this run's host-playground results in them.
//
//   COMMIT=<sha> RUN_URL=<url> node e2e/host-playground/release-notes.mjs <artifacts dir> <current notes file>
//
// The results replace any earlier section, so a rerun updates the notes rather than repeating them.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { attentionSections, summaryLine } from "./report.mjs";

const START = "<!-- host-playground-e2e -->";
const END = "<!-- /host-playground-e2e -->";

function section(artifactsDir, { commit, runUrl }) {
  const platforms = existsSync(artifactsDir) ? readdirSync(artifactsDir).sort() : [];
  const lines = [
    START,
    "### host-playground e2e",
    "",
    `Tested \`${commit.slice(0, 9)}\`, the commit this prerelease was built from, in a build with the e2e test code added. [Run](${runUrl}).`,
    "",
  ];
  for (const platform of platforms) {
    const results = join(artifactsDir, platform, "results.json");
    if (!existsSync(results)) {
      lines.push(`${platform.replace(/^host-playground-e2e-/, "")}: the run wrote no results.`, "");
      continue;
    }
    const run = JSON.parse(readFileSync(results, "utf8"));
    lines.push(summaryLine(run), "", ...attentionSections(run, "####"));
  }
  if (!platforms.length) lines.push("The run produced no results.", "");
  lines.push(END);
  return lines.join("\n");
}

function merge(notes, block) {
  const start = notes.indexOf(START);
  const end = notes.indexOf(END);
  if (start !== -1 && end > start) return `${notes.slice(0, start)}${block}${notes.slice(end + END.length)}`;
  return `${notes.trimEnd()}\n\n${block}\n`;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [artifactsDir, notesFile] = process.argv.slice(2);
  const { COMMIT: commit, RUN_URL: runUrl } = process.env;
  if (!artifactsDir || !notesFile || !commit || !runUrl) {
    console.error("usage: COMMIT=<sha> RUN_URL=<url> release-notes.mjs <artifacts dir> <current notes file>");
    process.exit(2);
  }
  process.stdout.write(merge(readFileSync(notesFile, "utf8"), section(artifactsDir, { commit, runUrl })));
}
