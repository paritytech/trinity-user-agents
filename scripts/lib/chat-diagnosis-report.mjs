/**
 * Decode a SCALE-encoded iOS Chat text message stored in CoreData.
 *
 * Mirrors the generated `ChatMessageContent` codec in `@parity/truapi`
 * (Text variant only) so this script stays runnable without building the
 * TS package first.
 */
export function decodeTextMessage(hex) {
  const encoded = Buffer.from(hex, "hex");
  if (encoded[0] !== 0) return undefined;
  const compact = decodeScaleCompact(encoded, 1);
  const start = 1 + compact.bytes;
  return encoded.subarray(start, start + compact.value).toString("utf8");
}

/** Title line the Chat diagnosis worker renders its report under. */
export const CHAT_DIAGNOSIS_HEADING = "## Truapi Chat Diagnosis";

/** Host gaps the Android launcher accepts: the shared core has no bot registry to register into. */
export const REGISTER_BOT_GAP = [
  { method: "Chat/register_bot", details: /no bot registry|not supported/ },
];

/** The `method` and `details` of every ❌ row in a report. */
export function diagnosisFailures(report) {
  const failures = [];
  for (const line of report.split("\n")) {
    const match = line.match(
      /^\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|\s*(.*?)\s*\|\s*$/,
    );
    if (match && match[2].includes("\u274c")) {
      failures.push({ method: match[1], details: match[3] });
    }
  }
  return failures;
}

/**
 * Validate a successful Chat report and attach the native host label.
 *
 * Throws unless every failed row is one of `acceptedFailures`.
 */
export function labelChatDiagnosisReport(
  report,
  host,
  { acceptedFailures = [] } = {},
) {
  const counts = report.match(/\*\*(\d+) success · (\d+) failed\*\*/);
  const failures = diagnosisFailures(report);
  const unexpected = failures.filter(
    (failure) =>
      !acceptedFailures.some(
        (accepted) =>
          accepted.method === failure.method &&
          accepted.details.test(failure.details),
      ),
  );
  const marked = report
    .split("\n")
    .filter((line) => line.includes("\u274c")).length;
  if (
    !report.startsWith(CHAT_DIAGNOSIS_HEADING) ||
    !counts ||
    counts[1] === "0" ||
    Number(counts[2]) !== failures.length ||
    unexpected.length > 0 ||
    marked !== failures.length
  ) {
    throw new Error(`Chat diagnosis reported a failure:\n${report}`);
  }
  const labelled = report.replace(
    CHAT_DIAGNOSIS_HEADING,
    `## Truapi ${host} Chat Diagnosis`,
  );
  if (failures.length === 0) {
    return labelled;
  }
  const accepted = failures
    .map((failure) => `${failure.method}: ${failure.details}`)
    .join("; ");
  return `${labelled}\n\n_Accepted host gaps: ${accepted}_`;
}

function decodeScaleCompact(encoded, offset) {
  const first = encoded[offset];
  const mode = first & 0b11;
  if (mode === 0) return { value: first >> 2, bytes: 1 };
  if (mode === 1) {
    return { value: encoded.readUInt16LE(offset) >> 2, bytes: 2 };
  }
  if (mode === 2) {
    return { value: encoded.readUInt32LE(offset) >>> 2, bytes: 4 };
  }
  throw new Error(
    "Large SCALE compact values are not expected in Chat reports",
  );
}
