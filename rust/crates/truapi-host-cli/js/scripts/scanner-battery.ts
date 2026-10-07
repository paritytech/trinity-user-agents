/// <reference path="../runner.ts" />
// Scanner checks against a real host, run by `scripts/battery.sh --scanner-host`.
// The host scans SCANNED_TEXT in every case and does not filter, so a refused
// code proves the core checked the host's answer. The product never signs in.
import type { HostScannerScanRequest } from "../../../../../js/packages/truapi/src/generated/types.ts";

const SCANNED_TEXT = "https://greenmarket.example/r/BAG6";

if (process.env.TRUAPI_SCAN_TEXT !== SCANNED_TEXT) {
  throw new Error(`scanner battery needs TRUAPI_SCAN_TEXT=${SCANNED_TEXT}`);
}

const SCANNED = { tag: "Scanned", value: { text: SCANNED_TEXT, format: "Qr" } };

const cases: [string, HostScannerScanRequest, string | typeof SCANNED][] = [
  [
    "a matching code reaches the product",
    { formats: ["Qr"], prefix: "https://greenmarket.example/r/" },
    SCANNED,
  ],
  [
    "the prefix ignores letter case",
    { formats: ["Qr"], prefix: "HTTPS://GREENMARKET.EXAMPLE/R/" },
    SCANNED,
  ],
  [
    "a code without the prefix is refused",
    { formats: ["Qr"], prefix: "polkadotapp://pair" },
    "Unknown",
  ],
  [
    "a format the product did not ask for is refused",
    { formats: ["Ean13"] },
    "Unknown",
  ],
  [
    "a two-line hint is refused before the host is asked",
    { formats: ["Qr"], hint: "Scan this\nto sign in" },
    "InvalidRequest",
  ],
  [
    "no formats is refused before the host is asked",
    { formats: [] },
    "InvalidRequest",
  ],
];

let failures = 0;
for (const [name, request, expected] of cases) {
  const result = await truapi.scanner.scan(request);
  const actual = result.isOk()
    ? result.value.outcome
    : result.error.tag === "Domain"
      ? result.error.value.value.tag
      : result.error.tag;
  const pass = JSON.stringify(actual) === JSON.stringify(expected);
  if (!pass) failures++;
  console.log(`${pass ? "✅" : "❌"} ${name}: ${JSON.stringify(actual)}`);
}

if (failures > 0) {
  throw new Error(
    `scanner battery failed: ${failures} of ${cases.length} cases`,
  );
}
console.log(`scanner battery: ${cases.length} cases passed`);
