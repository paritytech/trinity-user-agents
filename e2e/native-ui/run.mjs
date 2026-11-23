#!/usr/bin/env node
// Runs the native UI flows against an app that is installed and signed in. See README.md.
//
//   node e2e/native-ui/run.mjs --platform <android|ios> --app-id <id> --username <name> --out <dir> [--device <id>]

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { selectSimulator } from "../../scripts/lib/ios-simulator.mjs";

const FLOW = join(dirname(fileURLToPath(import.meta.url)), "smoke.yaml");

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
  { stdio: "inherit" },
);

const passed = result.status === 0;
writeFileSync(
  join(out, "report.md"),
  `### Native UI (${args.platform})\n\n${passed ? "Settings and wallet smoke passed." : "Settings and wallet smoke failed. The debug output in the uploaded results has a screenshot and the view hierarchy."}\n`,
);
process.exit(passed ? 0 : 1);
