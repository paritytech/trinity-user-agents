/// <reference path="../runner.ts" />
// Scanner checks against a real host, over the real wire.
//
// Run via:
//   scripts/battery.sh --scanner-host
//
// which starts a signing host with `TRUAPI_SCAN_TEXT` set. The product never
// signs in, because scanning reads no account.
import { runScannerE2e, SCANNED_TEXT } from "../scanner-e2e.ts";

if (process.env.TRUAPI_SCAN_TEXT !== SCANNED_TEXT) {
  throw new Error(
    `scanner battery needs TRUAPI_SCAN_TEXT=${SCANNED_TEXT}; run it through scripts/battery.sh --scanner-host`,
  );
}

const rows = await runScannerE2e(truapi);
for (const row of rows) {
  console.log(`${row.pass ? "✅" : "❌"} ${row.name}: ${row.output}`);
}

const failures = rows.filter((row) => !row.pass);
if (failures.length > 0) {
  throw new Error(
    `scanner battery failed: ${failures.length} of ${rows.length} cases\n${failures
      .map((row) => `${row.name}: ${row.output}`)
      .join("\n")}`,
  );
}

console.log(`scanner battery: ${rows.length} cases passed`);
