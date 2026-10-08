/// <reference path="../runner.ts" />
// Funding protocol check against a real host, over the real wire.
//
// Run via:
//   scripts/battery.sh --funding-host
//
// which builds the CLI with `--features test-host` and starts a signing host
// with a scripted funding overlay, so a product's requests are shown, started
// or dismissed, and settled with no chain behind them.
//
// The battery runs this script twice, as a Worker, around a host restart.
// `TRUAPI_FUNDING_STAGE=start` runs the product's cases and the provider's up
// to an inbound top-up, and keeps its rows in `TRUAPI_FUNDING_STATE`;
// `TRUAPI_FUNDING_STAGE=resume` finishes that session in the new process and
// writes the report with every row. Without a stage it runs the product's
// cases alone.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type { DiagnosisRow } from "../diagnosis.ts";
import { runFundingE2e } from "../funding-e2e.ts";
import {
  type ProviderState,
  runProviderResume,
  runProviderStart,
} from "../funding-provider-e2e.ts";
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

const fundingLog = process.env.TRUAPI_FUNDING_LOG;
const stage = process.env.TRUAPI_FUNDING_STAGE;
const statePath = process.env.TRUAPI_FUNDING_STATE;

const print = (rows: DiagnosisRow[]) => {
  for (const row of rows) {
    const mark = { pass: "✅", fail: "❌", skipped: "⏭️" }[row.status];
    console.log(`${mark} ${row.id} (${row.durationMs}ms) ${row.output}`);
  }
};

interface SavedRun {
  rows: DiagnosisRow[];
  provider?: ProviderState;
}

/** The rows the report holds: the saved first run plus the resumed one, or
 *  the product's cases alone without a stage. */
async function rowsToReport(): Promise<DiagnosisRow[]> {
  if (stage !== "resume") {
    const rows = await runFundingE2e(truapi, fundingLog);
    print(rows);
    return rows;
  }
  if (!statePath || !fundingLog) {
    throw new Error(
      "the resume stage needs TRUAPI_FUNDING_STATE and TRUAPI_FUNDING_LOG",
    );
  }
  const saved: SavedRun = existsSync(statePath)
    ? JSON.parse(readFileSync(statePath, "utf8"))
    : { rows: [] };
  const resumed = await runProviderResume(truapi, fundingLog, saved.provider);
  print(resumed);
  return [...saved.rows, ...resumed];
}

if (stage === "start") {
  if (!statePath || !fundingLog) {
    throw new Error(
      "the start stage needs TRUAPI_FUNDING_STATE and TRUAPI_FUNDING_LOG",
    );
  }
  const consumer = await runFundingE2e(truapi, fundingLog);
  const provider = await runProviderStart(truapi, fundingLog);
  const saved: SavedRun = {
    rows: [...consumer, ...provider.rows],
    provider: provider.state,
  };
  print(saved.rows);
  writeFileSync(statePath, JSON.stringify(saved));
  console.log(`funding battery: first run kept in ${statePath}`);
} else {
  finish(await rowsToReport());
}

/** Write the report, then fail on any skipped or failed case. */
function finish(rows: DiagnosisRow[]) {
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
}
