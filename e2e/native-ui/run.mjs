#!/usr/bin/env node
// Runs the native UI flows against an app that is installed and signed in. See README.md.
//
//   node e2e/native-ui/run.mjs --platform <android|ios> --app-id <id> --username <name> --out <dir> [--device <id>]

import { spawn, spawnSync } from "node:child_process";
import { closeSync, mkdirSync, openSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { selectSimulator } from "../../scripts/lib/ios-simulator.mjs";

const FLOW = join(dirname(fileURLToPath(import.meta.url)), "smoke.yaml");
// A passing run takes a few minutes; this stops a hung device from holding the job.
const FLOW_TIMEOUT_MS = 10 * 60_000;

const { values: args } = parseArgs({
  options: {
    platform: { type: "string" },
    "app-id": { type: "string" },
    username: { type: "string" },
    out: { type: "string" },
    device: { type: "string" },
  },
});
if (!["android", "ios"].includes(args.platform) || !args["app-id"] || !args.username || !args.out) {
  console.error(
    "usage: run.mjs --platform <android|ios> --app-id <id> --username <name> --out <dir> [--device <id>]",
  );
  process.exit(2);
}

const out = resolve(args.out);
mkdirSync(out, { recursive: true });

// The simulator the host-playground run used, chosen the same way.
const device = args.device ?? (args.platform === "ios" ? selectSimulator().udid : undefined);

const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
const adb = sdk ? join(sdk, "platform-tools", "adb") : "adb";

// Streamed to the host while the flow runs, so the log survives a device that stops answering.
const logcat = args.platform === "android" ? streamLogcat() : undefined;

const result = spawnSync(
  "maestro",
  [
    ...(device ? ["--device", device] : []),
    "test",
    "-e", `APP_ID=${args["app-id"]}`,
    "-e", `USERNAME=${args.username}`,
    "--format", "junit",
    "--output", join(out, "junit.xml"),
    "--debug-output", join(out, "debug"),
    "--flatten-debug-output",
    FLOW,
  ],
  { stdio: "inherit", timeout: FLOW_TIMEOUT_MS, killSignal: "SIGKILL" },
);

const timedOut = result.error?.code === "ETIMEDOUT";
const passed = result.status === 0;
logcat?.kill();
if (!passed && args.platform === "android") saveAndroidDiagnostics();
writeFileSync(
  join(out, "report.md"),
  `### Native UI (${args.platform})\n\n${summary()}\n`,
);
process.exit(passed ? 0 : 1);

function summary() {
  if (passed) return "Settings and wallet smoke passed.";
  if (timedOut) return `Settings and wallet smoke timed out after ${FLOW_TIMEOUT_MS / 60_000} minutes.`;
  return "Settings and wallet smoke failed. The debug output in the uploaded results has a screenshot and the view hierarchy.";
}

function streamLogcat() {
  const file = openSync(join(out, "logcat.txt"), "w");
  spawnSync(adb, ["logcat", "-c"], { timeout: 10_000 });
  const child = spawn(adb, ["logcat", "-b", "all", "-v", "threadtime"], { stdio: ["ignore", file, file] });
  closeSync(file);
  return child;
}

/** Keeps what the device saw, since a Maestro driver failure leaves no screenshot of its own. */
function saveAndroidDiagnostics() {
  // Bounded, since adb waits forever on a device that has gone offline.
  const save = (name, command, encoding = "utf8") => {
    const { stdout } = spawnSync(adb, command, { encoding, maxBuffer: 256 * 1024 * 1024, timeout: 20_000 });
    if (stdout?.length) writeFileSync(join(out, name), stdout);
  };
  save("screen.png", ["exec-out", "screencap", "-p"], "buffer");
  save("meminfo.txt", ["shell", "dumpsys", "meminfo"]);
  save("gfxinfo.txt", ["shell", "dumpsys", "gfxinfo", args["app-id"]]);
}
