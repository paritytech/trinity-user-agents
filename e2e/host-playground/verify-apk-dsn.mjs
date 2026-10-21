import { readFileSync, readdirSync, unlinkSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

function findAapt() {
  const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
  const buildTools = sdk && join(sdk, "build-tools");
  if (buildTools) {
    try {
      const versions = readdirSync(buildTools).sort((left, right) => {
        const a = left.split(".").map(Number);
        const b = right.split(".").map(Number);
        for (let index = 0; index < Math.max(a.length, b.length); index++) {
          const difference = (b[index] || 0) - (a[index] || 0);
          if (difference) return difference;
        }
        return 0;
      });
      for (const version of versions) {
        const path = join(buildTools, version, process.platform === "win32" ? "aapt.exe" : "aapt");
        const result = spawnSync(path, ["version"], { encoding: "utf8" });
        if (result.status === 0) return path;
      }
    } catch {
      // Fall through to PATH.
    }
  }
  return "aapt";
}

function manifestDsn(xmltree) {
  const lines = xmltree.split("\n");
  const entry = lines.findIndex((line) => line.includes('io.sentry.dsn'));
  if (entry < 0) return null;
  for (const line of lines.slice(entry, entry + 5)) {
    const value = line.match(/android:value[^=]*="([^"]*)"/);
    if (value) return value[1];
  }
  return null;
}

const apk = process.argv[2];
let expected;
try {
  expected = readFileSync(process.env.SENTRY_DSN_FILE, "utf8");
} catch {
  throw new Error("resolved Android Sentry DSN file is missing");
}
if (!apk) throw new Error("APK path is required");
if (!expected) throw new Error("resolved Android Sentry DSN is empty");

const result = spawnSync(findAapt(), ["dump", "xmltree", apk, "AndroidManifest.xml"], { encoding: "utf8" });
if (result.status !== 0) throw new Error("could not inspect the Android APK manifest");
const actual = manifestDsn(result.stdout);
if (!actual) throw new Error("the Android APK has no Sentry DSN metadata");
if (actual !== expected) throw new Error("the Android APK Sentry DSN does not match SENTRY_DSN_ANDROID");

try {
  const url = new URL(actual);
  const isSentryHost = url.hostname === "sentry.io" || url.hostname.endsWith(".sentry.io");
  if (!isSentryHost || !/^\/[0-9]+$/.test(url.pathname)) {
    throw new Error("invalid DSN shape");
  }
} catch {
  throw new Error("the Android APK Sentry DSN is invalid");
}

console.log("APK Sentry DSN matches the configured Android project");
unlinkSync(process.env.SENTRY_DSN_FILE);
