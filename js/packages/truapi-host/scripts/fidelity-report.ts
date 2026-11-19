// Run the generated battery against the mock host and write its diagnosis
// report, in the same shape the CLI host roles write theirs.
//
// The point is comparison, not a pass count. `explorer/diagnosis-reports/spa/`
// already holds one report per host; adding the mock's makes "does a product
// see the same thing here as against a real host" a diff rather than an
// argument. Drift shows up as a change to the committed report.
//
// Chain calls are closed rather than left silent so the battery terminates: a
// silent chain parks every chain-routed method until the process is killed.
// Those methods therefore fail HERE for a harness reason, which is why
// `diagnosis-reports.test.ts` compares agreement rather than counting passes.
//
// Run by hand. Nothing in CI regenerates this report, so the committed one is
// only as current as the last run, and `diagnosis-reports.test.ts` compares
// committed files rather than live behaviour.
//
//   bun js/packages/truapi-host/scripts/fidelity-report.ts
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockClient } from "../src/testing/create-mock-client.ts";
import { renderDiagnosisReport } from "../../../../rust/crates/truapi-host-cli/js/diagnosis-report.ts";
import { runDiagnosis } from "../../../../rust/crates/truapi-host-cli/js/diagnosis.ts";

const REPORT_PATH = fileURLToPath(
  new URL(
    "../../../../explorer/diagnosis-reports/spa/mock-host.md",
    import.meta.url,
  ),
);

const { client, dispose } = await createMockClient({
  mock: { chainClosed: true },
  runtimeConfig: { productId: "truapi-playground.dot" },
});

const rows = await runDiagnosis(client, {
  runKnownUnsupported: true,
  onResult: (_row, index, total) => {
    if (index % 10 === 0) console.error(`  ${index}/${total}`);
  },
});
dispose();

mkdirSync(dirname(REPORT_PATH), { recursive: true });
writeFileSync(
  REPORT_PATH,
  renderDiagnosisReport("TrUAPI Mock Host Diagnosis", rows),
);
console.error(`wrote ${REPORT_PATH}`);
