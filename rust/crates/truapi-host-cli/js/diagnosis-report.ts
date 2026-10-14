import type { DiagnosisRow } from "./diagnosis.ts";

export type CliHostRole = "pairing-host" | "signing-host";

export interface CliDiagnosisReportMetadata {
  filename: "pairing-host-cli.md" | "signing-host-cli.md";
  title:
    | "Truapi Pairing Host CLI Diagnosis"
    | "Truapi Signing Host CLI Diagnosis";
}

/** Select the committed report identity from the host that serves the script. */
export function cliDiagnosisReportMetadata(
  role: string | undefined,
): CliDiagnosisReportMetadata {
  switch (role) {
    case "pairing-host":
      return {
        filename: "pairing-host-cli.md",
        title: "Truapi Pairing Host CLI Diagnosis",
      };
    case "signing-host":
      return {
        filename: "signing-host-cli.md",
        title: "Truapi Signing Host CLI Diagnosis",
      };
    default:
      throw new Error(
        `TRUAPI_CLI_HOST_ROLE must be pairing-host or signing-host; received ${JSON.stringify(role)}`,
      );
  }
}

/**
 * Report metadata for the same CLI host serving a Chat execution.
 *
 * The aggregator picks the matrix from the parent directory, so a chat report
 * belongs under `diagnosis-reports/chat/`; the title carries the modality too,
 * matching `chat/ios.md`.
 */
export function cliChatDiagnosisReportMetadata(
  role: string | undefined,
): CliDiagnosisReportMetadata {
  const spa = cliDiagnosisReportMetadata(role);
  return {
    filename: spa.filename,
    title: spa.title.replace(/ Diagnosis$/, " Chat Diagnosis"),
  };
}

/**
 * Report metadata for the same CLI host serving a Pocket execution.
 *
 * The aggregator picks the matrix from the parent directory, so a Pocket
 * report belongs under `diagnosis-reports/pocket/`; the title carries the
 * modality too.
 */
export function cliPocketDiagnosisReportMetadata(
  role: string | undefined,
): CliDiagnosisReportMetadata {
  const spa = cliDiagnosisReportMetadata(role);
  return {
    filename: spa.filename,
    title: spa.title.replace(/ Diagnosis$/, " Pocket Diagnosis"),
  };
}

/** Render the same Markdown matrix used by the playground diagnosis reports. */
export function renderDiagnosisReport(
  title: string,
  rows: DiagnosisRow[],
): string {
  return (
    [
      `## ${title}`,
      "",
      "| Method | Status | Details |",
      "| --- | --- | --- |",
      ...rows.map(
        (row) =>
          `| \`${row.id}\` | ${statusIcon(row.status)} | ${
            row.status === "pass" ? "" : cleanMarkdownDetail(row.output)
          } |`,
      ),
    ].join("\n") + "\n"
  );
}

function statusIcon(status: DiagnosisRow["status"]): string {
  if (status === "pass") return "✅";
  if (status === "skipped") return "⏭️";
  return "❌";
}

export function cleanMarkdownDetail(output: string): string {
  const collapsed = output
    .replace(/\u001b\[[0-9;]*m/g, "")
    .replace(/\s+/g, " ")
    .trim();
  const concise = truncateMiddle(collapsed, 300);
  return concise.replaceAll("|", "\\|");
}

function truncateMiddle(value: string, limit: number): string {
  if (value.length <= limit) return value;
  const tailLength = Math.ceil(limit * 0.6);
  const headLength = limit - tailLength - 3;
  return `${value.slice(0, headLength)}...${value.slice(-tailLength)}`;
}
