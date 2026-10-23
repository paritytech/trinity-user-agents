/// <reference path="../runner.ts" />
// Chat content screening against a real host, over the real wire.
//
// Run via:
//   scripts/battery.sh --chat-host
//
// which starts a signing host with `--execution-kind worker`, so the product
// connection opens as a Worker execution and the CLI's in-memory chat host is
// installed. Chat is denied to an App connection and to a host with no session,
// so neither the generated App battery nor an in-process harness can reach
// this path: a live host is the only way to exercise it.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { runChatScreeningE2e } from "../chat-e2e.ts";
import {
  cliChatDiagnosisReportMetadata,
  renderDiagnosisReport,
} from "../diagnosis-report.ts";

const report = cliChatDiagnosisReportMetadata(process.env.TRUAPI_CLI_HOST_ROLE);
const DEFAULT_REPORT_PATH = fileURLToPath(
  new URL(
    `../../../../../explorer/diagnosis-reports/chat/${report.filename}`,
    import.meta.url,
  ),
);
const REPORT_PATH =
  process.env.TRUAPI_BATTERY_REPORT_PATH || DEFAULT_REPORT_PATH;

const login = await truapi.account.requestLogin({ reason: undefined });
if (
  !login.isOk() ||
  !["Success", "AlreadyConnected"].includes(String(login.value))
) {
  throw new Error(
    `chat battery login failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`,
  );
}

const rows = await runChatScreeningE2e(truapi, process.env.TRUAPI_CHAT_LOG);
for (const row of rows) {
  const mark = { pass: "✅", fail: "❌", skipped: "⏭️" }[row.status];
  console.log(`${mark} ${row.id} (${row.durationMs}ms) ${row.output}`);
}

// Committed, so a rerun overwrites it and the diff shows what changed. The
// chat matrix reads it from the directory it lands in.
mkdirSync(dirname(REPORT_PATH), { recursive: true });
writeFileSync(REPORT_PATH, renderDiagnosisReport(report.title, rows));
console.log(`chat battery: report saved to ${REPORT_PATH}`);

const skipped = rows.filter((row) => row.status === "skipped");
if (skipped.length > 0) {
  // A skip here means the run could not tell a core rejection from a host one,
  // which is the single thing these cases exist to distinguish.
  throw new Error(
    `chat battery skipped ${skipped.length} case(s): ${skipped
      .map((row) => row.output)
      .join("; ")}`,
  );
}

const failures = rows.filter((row) => row.status === "fail");
if (failures.length > 0) {
  throw new Error(
    `chat battery failed: ${failures.length} of ${rows.length} cases\n${failures
      .map((row) => `${row.id}: ${row.output}`)
      .join("\n")}`,
  );
}

console.log(`chat battery: ${rows.length} cases passed`);
