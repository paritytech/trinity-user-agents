// Days left on the iOS certificates and profiles validate-signing-credentials
// fetched.
//
//   bun ci-dashboard/src/run-signing.ts <certificates.pem> <profile dir>...
//
// SIGNING_APP_IDENTIFIERS, comma-separated, limits the profiles to those apps.
// Adds a table to the job summary, a warning for each item under 30 days, and
// exits non-zero while anything has under 7 days. It reads only public
// certificates and profiles, never a private key. Missing profile directories
// are skipped, so it runs locally on a folder of .mobileprovision files and
// any PEM.

import { appendFile, readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

import {
  currentProfiles,
  daysLeft,
  EXPIRY_THRESHOLDS,
  parseCertificates,
  parseProfile,
  summary,
} from "./signing";

const [pemPath, ...profileDirs] = process.argv.slice(2);
if (!pemPath) {
  throw new Error("usage: run-signing.ts <certificates.pem> <profile dir>...");
}

const now = new Date();
const certificates = parseCertificates(await readFile(pemPath, "utf8"));
const parsed = [];
for (const dir of profileDirs) {
  const files = await readdir(dir).catch(() => []);
  for (const file of files.filter((f) => f.endsWith(".mobileprovision"))) {
    const profile = parseProfile(await readFile(join(dir, file)));
    if (profile) {
      parsed.push(profile);
    }
  }
}
// The bundle identifiers match fetched; empty keeps every match profile.
const appIds = (process.env.SIGNING_APP_IDENTIFIERS ?? "")
  .split(",")
  .map((id) => id.trim())
  .filter(Boolean);
const items = [...certificates, ...currentProfiles(parsed, appIds)];
if (certificates.length === 0 || items.length === certificates.length) {
  // match succeeded, so finding nothing means this script is reading the
  // wrong place, which must not pass for "nothing expiring".
  throw new Error(
    `found ${certificates.length} certificate(s) and ${items.length - certificates.length} profile(s)`,
  );
}

const table = summary(items, now);
console.log(table);
if (process.env.GITHUB_STEP_SUMMARY) {
  await appendFile(process.env.GITHUB_STEP_SUMMARY, table);
}

const fail = EXPIRY_THRESHOLDS[EXPIRY_THRESHOLDS.length - 1];
for (const item of items) {
  const days = daysLeft(item, now);
  const date = item.expires.toISOString().slice(0, 10);
  if (days < fail) {
    console.log(`::error::${item.name} expires on ${date}; renew it now`);
  } else if (days < EXPIRY_THRESHOLDS[0]) {
    console.log(`::warning::${item.name} has ${days} days left (${date})`);
  }
}
const urgent = items.filter((item) => daysLeft(item, now) < fail);
if (urgent.length > 0) {
  console.error(`${urgent.length} item(s) with under ${fail} days left.`);
  process.exitCode = 1;
}
