/// <reference path="../runner.ts" />
// Funding protocol check against a real host, over the real wire.
//
// Run via:
//   scripts/battery.sh --funding-host
//
// which builds the CLI with `--features test-host` and starts a signing host
// with a scripted funding overlay, so a product's requests are shown, started
// or dismissed, and settled with no chain behind them.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { runFundingE2e } from "../funding-e2e.ts";
import {
  cliModalityDiagnosisReportMetadata,
  renderDiagnosisReport,
} from "../diagnosis-report.ts";

const report = cliModalityDiagnosisReportMetadata(
  process.env.TRUAPI_CLI_HOST_ROLE,
  "Funding",
);
const DEFAULT_REPORT_PATH = fileURLToPath(
  new URL(
    `../../../../../explorer/diagnosis-reports/funding/${report.filename}`,
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
    `funding battery login failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`,
  );
}

const rows = await runFundingE2e(truapi, process.env.TRUAPI_FUNDING_LOG);
for (const row of rows) {
  const mark = { pass: "✅", fail: "❌", skipped: "⏭️" }[row.status];
  console.log(`${mark} ${row.id} (${row.durationMs}ms) ${row.output}`);
}

// Committed, so a rerun overwrites it and the diff shows what changed. The
// explorer's Funding matrix reads it from the `funding/` directory it lands in.
mkdirSync(dirname(REPORT_PATH), { recursive: true });
writeFileSync(REPORT_PATH, renderDiagnosisReport(report.title, rows));
console.log(`funding battery: report saved to ${REPORT_PATH}`);

const skipped = rows.filter((row) => row.status === "skipped");
if (skipped.length > 0) {
  // A skip here means the run could not read what the host observed, which is
  // half of what these cases assert.
  throw new Error(
    `funding battery skipped ${skipped.length} case(s): ${skipped
      .map((row) => row.output)
      .join("; ")}`,
  );
}

const failures = rows.filter((row) => row.status === "fail");
if (failures.length > 0) {
  throw new Error(
    `funding battery failed: ${failures.length} of ${rows.length} cases\n${failures
      .map((row) => `${row.id}: ${row.output}`)
      .join("\n")}`,
  );
}

console.log(`funding battery: ${rows.length} cases passed`);
