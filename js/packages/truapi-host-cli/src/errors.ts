// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Error translation the core does not (yet) provide.
//
// A wallet that never answers an SSO request produces, after ~180 seconds, a
// BARE `{isSdkError: true, source: "tx", name: "TxError"}` with no message.
// The core's own diagnosis ("SSO response timed out") exists only as a
// `tracing` warning, so a product cannot tell "phone never answered" from any
// other transaction failure. Until the timeout becomes a typed error variant
// upstream, a bare `TxError` is treated as a probable timeout.

/** Whether `error` looks like the wallet-SSO 180s timeout. */
export function isProbableSsoTimeout(error: unknown): boolean {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  const candidate = error as {
    isSdkError?: unknown;
    source?: unknown;
    name?: unknown;
    message?: unknown;
  };
  return (
    candidate.isSdkError === true &&
    candidate.source === "tx" &&
    candidate.name === "TxError" &&
    (candidate.message === undefined ||
      candidate.message === null ||
      candidate.message === "")
  );
}

/**
 * A user-facing explanation for errors a product call can surface through the
 * core, or `undefined` when there is nothing better than the error itself.
 */
export function explainProductError(error: unknown): string | undefined {
  if (isProbableSsoTimeout(error)) {
    return (
      "No response from your phone. The signing request most likely timed " +
      "out. Open the Polkadot app, check for a pending request, and retry."
    );
  }
  return undefined;
}
