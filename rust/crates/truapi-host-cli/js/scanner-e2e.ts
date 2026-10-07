// End-to-end scanner checks, run against a CLI host that answers every scan
// with `TRUAPI_SCAN_TEXT` as a QR code.
//
// The CLI does not filter, so every case that refuses a code proves the core
// checked the host's answer, and every case that refuses a request proves the
// core checked it before asking the host.
import type { TrUApiClient } from "../../../../js/packages/truapi/src/index.ts";
import type {
  CodeFormat,
  HostScannerScanRequest,
} from "../../../../js/packages/truapi/src/generated/types.ts";

/** What the host scans in every case. */
export const SCANNED_TEXT = "https://greenmarket.example/r/BAG6";

/** One case and how it ended. */
export interface ScannerRow {
  name: string;
  pass: boolean;
  output: string;
}

type Expected =
  | { scanned: { text: string; format: CodeFormat } }
  | { domainError: "InvalidRequest" | "Unknown" };

interface Case {
  name: string;
  request: HostScannerScanRequest;
  expected: Expected;
}

const CASES: Case[] = [
  {
    name: "a code that matches the request reaches the product",
    request: {
      formats: ["Qr"],
      prefix: "https://greenmarket.example/r/",
      hint: "Point at the receipt's QR code",
    },
    expected: { scanned: { text: SCANNED_TEXT, format: "Qr" } },
  },
  {
    name: "the prefix ignores letter case",
    request: { formats: ["Qr"], prefix: "HTTPS://GREENMARKET.EXAMPLE/R/" },
    expected: { scanned: { text: SCANNED_TEXT, format: "Qr" } },
  },
  {
    name: "a code without the prefix never reaches the product",
    request: { formats: ["Qr"], prefix: "polkadotapp://pair" },
    expected: { domainError: "Unknown" },
  },
  {
    name: "a code in a format the product did not ask for never reaches it",
    request: { formats: ["Ean13"] },
    expected: { domainError: "Unknown" },
  },
  {
    name: "a hint on two lines is refused before the host is asked",
    request: { formats: ["Qr"], hint: "Scan the code on your computer\nto sign in" },
    expected: { domainError: "InvalidRequest" },
  },
  {
    name: "a request with no formats is refused before the host is asked",
    request: { formats: [] },
    expected: { domainError: "InvalidRequest" },
  },
];

/** Run every case against `truapi`, which must not be signed in. */
export async function runScannerE2e(truapi: TrUApiClient): Promise<ScannerRow[]> {
  const rows: ScannerRow[] = [];
  for (const { name, request, expected } of CASES) {
    const result = await truapi.scanner.scan(request);
    const actual = result.isOk()
      ? JSON.stringify(result.value.outcome)
      : JSON.stringify(result.error);
    const pass =
      "scanned" in expected
        ? result.isOk() &&
          result.value.outcome.tag === "Scanned" &&
          result.value.outcome.value.text === expected.scanned.text &&
          result.value.outcome.value.format === expected.scanned.format
        : result.isErr() &&
          result.error.tag === "Domain" &&
          result.error.value.value.tag === expected.domainError;
    rows.push({ name, pass, output: actual });
  }
  return rows;
}
