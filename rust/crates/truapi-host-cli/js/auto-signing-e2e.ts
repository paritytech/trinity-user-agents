// End-to-end AutoSigning check (RFC 0010 + RFC 0023): after an approved
// `AutoSigning` allocation, `sign_vrf` for the granting product must be served
// without consulting the host's confirmation prompt.
//
// The host CLI appends one `<approved|denied> <action>` line per decided
// confirmation to `TRUAPI_APPROVALS_LOG` (`scripts/battery.sh` exports it, and
// in the paired phase both host processes inherit and share the file). A line
// is written before the confirmation resolves, so any prompt a product call
// consulted is on disk by the time the call returns: an empty `sign_vrf`
// window proves no prompt fired anywhere.
import { existsSync, readFileSync } from "node:fs";
import type { TrUApiClient } from "../../../../js/packages/truapi/src/index.ts";
import type { DiagnosisRow } from "./diagnosis.ts";

export const VRF_APPROVAL_ACTION = "sign VRF transcript";
export const ALLOCATION_APPROVAL_ACTION = "allocate resources";
export const SIGN_RAW_APPROVAL_ACTION = "sign raw data";
export const SIGN_PAYLOAD_APPROVAL_ACTION = "sign payload";
export const CREATE_TRANSACTION_APPROVAL_ACTION = "create transaction";

/** Non-empty transcript lines, oldest first. */
export function approvalLines(text: string): string[] {
  return text.split("\n").filter((line) => line.length > 0);
}

/** Lines appended after the `before` snapshot was taken. */
export function newLinesSince(before: string[], after: string[]): string[] {
  return after.slice(before.length);
}

/** Lines whose confirmation action matches `action`. */
export function actionLines(lines: string[], action: string): string[] {
  return lines.filter((line) => line.includes(action));
}

/**
 * Allocate `AutoSigning`, then require two `sign_vrf` calls for the granting
 * product to succeed while appending zero VRF confirmation lines to the
 * approvals transcript. Reported as one extra diagnosis row.
 */
export async function runAutoSigningE2e(
  client: TrUApiClient,
  productId: string,
  approvalsLogPath: string | undefined,
): Promise<DiagnosisRow> {
  const startedAt = performance.now();
  const finish = (
    status: DiagnosisRow["status"],
    output: string,
  ): DiagnosisRow => ({
    id: "Resource Allocation/auto_signing_e2e",
    serviceName: "Resource Allocation",
    methodName: "auto_signing_e2e",
    status,
    output,
    durationMs: Math.round(performance.now() - startedAt),
  });

  if (!approvalsLogPath) {
    return finish(
      "skipped",
      "TRUAPI_APPROVALS_LOG not set; cannot verify prompt-free sign_vrf",
    );
  }
  const readTranscript = () =>
    existsSync(approvalsLogPath)
      ? approvalLines(readFileSync(approvalsLogPath, "utf8"))
      : [];

  try {
    const beforeAllocation = readTranscript();
    const allocation = await client.resourceAllocation.request({
      resources: [{ tag: "AutoSigning" }],
    });
    if (!allocation.isOk()) {
      return finish(
        "fail",
        `AutoSigning allocation failed: ${JSON.stringify(allocation.error)}`,
      );
    }
    const outcome = allocation.value.outcomes[0];
    if (outcome !== "Allocated") {
      return finish("fail", `AutoSigning was not allocated: ${outcome}`);
    }
    // The allocation consent must be visible in the transcript; otherwise the
    // empty sign_vrf windows below would prove nothing.
    const allocationWindow = newLinesSince(beforeAllocation, readTranscript());
    if (
      actionLines(allocationWindow, ALLOCATION_APPROVAL_ACTION).length === 0
    ) {
      return finish(
        "fail",
        "approvals transcript recorded no allocation consent; " +
          "prompt tracking is not wired up",
      );
    }

    // Two calls: the first exercises grant lookup (and, on a pairing host,
    // capability restore from core storage), the second the in-memory cache.
    for (const round of [1, 2]) {
      const beforeVrf = readTranscript();
      const vrf = await client.account.signVrf({
        account: {
          dotNsIdentifier: productId,
          derivationIndex: { tag: "Index", value: 0 },
        },
        transcriptLabel: "0x706f703a61697264726f70",
        items: [
          { label: "0x646f6d61696e", value: "0x706f703a61697264726f70" },
          { label: "0x7369676e6572", value: "0x00" },
        ],
      });
      if (!vrf.isOk()) {
        return finish(
          "fail",
          `sign_vrf round ${round} failed: ${JSON.stringify(vrf.error)}`,
        );
      }
      const prompts = actionLines(
        newLinesSince(beforeVrf, readTranscript()),
        VRF_APPROVAL_ACTION,
      );
      if (prompts.length > 0) {
        return finish(
          "fail",
          `sign_vrf round ${round} consulted a confirmation despite the ` +
            `AutoSigning grant: ${prompts.join("; ")}`,
        );
      }
    }
    // The RFC-0023 VRF path is one of four the grant covers. The rest are
    // product-account signing APIs, each asserted the same way: open a
    // transcript window, call, and require the call's own action to be absent.
    const account = {
      dotNsIdentifier: productId,
      derivationIndex: { tag: "Index", value: 0 } as const,
    };
    /** Lines for `action` appended since `before`. Empty means no prompt fired. */
    const promptsSince = (before: string[], action: string): string[] =>
      actionLines(newLinesSince(before, readTranscript()), action);

    /**
     * Run one covered call and require it to append no line for `action`.
     * Returns an error row, or null when the call was served prompt-free.
     */
    const requireNoPrompt = async (
      name: string,
      action: string,
      call: () => Promise<{ isOk: () => boolean; error: unknown }>,
    ): Promise<DiagnosisRow | null> => {
      const before = readTranscript();
      const result = await call();
      if (!result.isOk()) {
        return finish(
          "fail",
          `${name} failed: ${JSON.stringify(result.error)}`,
        );
      }
      const prompts = promptsSince(before, action);
      return prompts.length > 0
        ? finish(
            "fail",
            `${name} consulted a confirmation despite the AutoSigning grant: ${prompts.join("; ")}`,
          )
        : null;
    };

    const failure =
      (await requireNoPrompt("sign_raw", SIGN_RAW_APPROVAL_ACTION, () =>
        client.signing.signRaw({
          account,
          payload: { tag: "Bytes", value: { bytes: "0x48656c6c6f" } },
        }),
      )) ??
      (await requireNoPrompt("sign_payload", SIGN_PAYLOAD_APPROVAL_ACTION, () =>
        // Optional fields are absent rather than null: the codec wraps each in
        // `Option`, so a null reaches the hex encoder as a value.
        client.signing.signPayload({
          account,
          payload: {
            blockHash: `0x${"00".repeat(32)}`,
            blockNumber: "0x00000000",
            era: "0x0000",
            genesisHash: `0x${"00".repeat(32)}`,
            method: "0x00003448656c6c6f2c20776f726c6421",
            nonce: "0x00000000",
            signedExtensions: [],
            specVersion: "0x00000000",
            tip: "0x00000000000000000000000000000000",
            transactionVersion: "0x00000000",
            version: 4,
          },
        }),
      )) ??
      (await requireNoPrompt(
        "create_transaction",
        CREATE_TRANSACTION_APPROVAL_ACTION,
        () =>
          // V4 is assembled offline, so the grant is the only variable here.
          client.signing.createTransaction({
            signer: account,
            genesisHash: `0x${"01".repeat(32)}`,
            callData: "0x0400",
            extensions: [],
            txExtVersion: 0,
          }),
      ));
    if (failure) {
      return failure;
    }

    return finish(
      "pass",
      "AutoSigning allocated with consent; 2 sign_vrf calls plus sign_raw, " +
        "sign_payload and create_transaction served without a confirmation prompt",
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return finish("fail", message);
  }
}
