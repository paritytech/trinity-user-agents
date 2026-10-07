// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Confirm-prompt content for the core's review surface.
//
// The prompts are DELIBERATELY modest. The host cannot decode what it is
// approving: `CreateTransaction` carries `callData` as opaque hex without a
// method name, a readable address, or arguments, and `PreimageSubmit`
// carries only a byte count. The paired wallet is the authoritative trust
// surface. It decodes and displays the real content before anything is
// signed. So `confirmUserAction` is a local pre-confirmation, and these
// prompts render only what the host knows on TYPED authority (review kind,
// account, chain, sizes) and defer the content to the phone rather than
// pretending to summarise bytes they cannot decode.

import type { UserConfirmationReview } from "@parity/truapi-host";
import type { ChainEndpoints } from "./chain-pool.js";
import { hexByteLength, shortHex } from "./hex.js";

/** What a presenter is asked to confirm. */
export interface ConfirmRequest {
  /** One-line action statement, e.g. `Sign a message`. */
  title: string;
  /** Typed metadata the host knows on its own authority. */
  details: string[];
  /**
   * Whether the paired wallet is the authoritative surface for this action.
   * When true, presenters tell the user to complete it on the phone. The
   * default phrasing is signing-specific ("nothing is signed until you
   * approve it there"). Reviews that are NOT signing operations should set
   * {@link ConfirmRequest.phoneNote} so the prompt does not claim otherwise.
   */
  phoneVerifies: boolean;
  /**
   * Overrides the default signing-specific "defer to the phone" line, and
   * renders regardless of {@link ConfirmRequest.phoneVerifies}. Set it
   * wherever the default would mislead: authority or read requests where
   * nothing is ever signed (e.g. resolving account keys), and locally-signed
   * actions (Bulletin writes under an allowance) where terminal approval is
   * FINAL and no phone checkpoint follows.
   */
  phoneNote?: string;
}

function chainName(
  endpoints: ChainEndpoints | undefined,
  genesisHash: string,
): string {
  const name = endpoints?.[genesisHash]?.name;
  return name !== undefined ? name : `chain ${genesisHash.slice(0, 10)}…`;
}

function accountLine(account: {
  dotNsIdentifier: string;
  derivationIndex:
    { tag: "Index"; value: number } | { tag: "Raw"; value: string };
}): string {
  const index =
    account.derivationIndex.tag === "Index"
      ? `#${String(account.derivationIndex.value)}`
      : shortHex(account.derivationIndex.value);
  return `account: ${account.dotNsIdentifier} (derivation ${index})`;
}

/**
 * The requesting product, on the signing reviews where the core attributes
 * the request to a caller (since 0.23). Absent attribution renders nothing
 * rather than a placeholder.
 */
function callerLines(callingProductId: string | undefined): string[] {
  return callingProductId === undefined ? [] : [`product: ${callingProductId}`];
}

/** The wallet decodes legacy-account requests too. The host knows even less. */
function legacyAccountReview(title: string): ConfirmRequest {
  return { title, details: [], phoneVerifies: true };
}

/**
 * Describe a review for a terminal confirm prompt.
 *
 * `endpoints` (optional) resolves genesis hashes to human-readable chain
 * names in transaction prompts.
 */
export function describeReview(
  review: UserConfirmationReview,
  options: { endpoints?: ChainEndpoints } = {},
): ConfirmRequest {
  switch (review.tag) {
    case "SignRaw": {
      if (review.value.tag !== "Product") {
        return legacyAccountReview("Sign a message with a legacy account");
      }
      const { callingProductId, request, watermarked } = review.value.value;
      const { account, payload } = request;
      // The RawPayload discriminant survives into the review: `Bytes` is
      // raw binary, `Payload` is a wrapped string message. Since 0.16 the
      // review also states whether the core applies the `<Bytes>` watermark
      // envelope before signing. Rendering both matters: an unwatermarked
      // signature over raw bytes could authorize anything, including a
      // transaction-shaped payload.
      const kind =
        payload.tag === "Bytes"
          ? `raw binary data (${String(hexByteLength(payload.value.bytes))} bytes)`
          : "a text message";
      return {
        title: "Sign a message",
        details: [
          ...callerLines(callingProductId),
          accountLine(account),
          `payload: ${kind}`,
          watermarked
            ? "wrapped in the <Bytes> envelope before signing"
            : "signed as-is (no <Bytes> envelope): the bytes could encode anything, including a transaction",
        ],
        phoneVerifies: true,
      };
    }
    case "SignPayload": {
      if (review.value.tag !== "Product") {
        return legacyAccountReview("Sign a payload with a legacy account");
      }
      const { callingProductId, request } = review.value.value;
      return {
        title: "Sign a payload",
        details: [
          ...callerLines(callingProductId),
          accountLine(request.account),
        ],
        phoneVerifies: true,
      };
    }
    case "CreateTransaction": {
      if (review.value.tag !== "Product") {
        return legacyAccountReview(
          "Submit a transaction with a legacy account",
        );
      }
      const { callingProductId, payload } = review.value.value;
      const { signer, genesisHash, callData } = payload;
      return {
        title: `Submit a transaction on ${chainName(options.endpoints, genesisHash)}`,
        details: [
          ...callerLines(callingProductId),
          accountLine(signer),
          `call data: ${String(hexByteLength(callData))} bytes (not decodable here)`,
        ],
        phoneVerifies: true,
      };
    }
    case "StatementStoreProductSign":
      // The payload is the exact unsigned statement, signed as-is (no
      // `<Bytes>` envelope), so it must NOT be presented with the
      // raw-message-signing convention.
      return {
        title: "Sign a Statement Store proof",
        details: [
          ...callerLines(review.value.callingProductId),
          accountLine(review.value.account),
          `statement payload: ${String(review.value.payload.length)} bytes`,
        ],
        phoneVerifies: true,
      };
    case "SignVrf":
      return {
        title: "Sign a VRF transcript",
        details: [
          `product: ${review.value.callingProductId}`,
          accountLine(review.value.request.account),
          `transcript items: ${String(review.value.request.items.length)}`,
        ],
        phoneVerifies: true,
      };
    case "ResourceAllocation":
      return {
        title: "Allocate network resources",
        details: [
          `for product: ${review.value.callingProductId}`,
          ...review.value.resources.map(
            (resource) => `resource: ${JSON.stringify(resource)}`,
          ),
        ],
        phoneVerifies: true,
      };
    case "ProductSubtree":
      // Resolves the product's own account subtree over SSO. The answer is a
      // public key, and addresses derived from it appear on later reviews. It is
      // a read-authority request, NOT signing, so the note must not claim a
      // signing prompt is coming.
      return {
        title: "Let this app resolve its account address",
        details: [`product: ${review.value.productId}`],
        phoneVerifies: true,
        phoneNote:
          "Approve on your phone to let this app read its account keys.",
      };
    case "PreimageSubmit":
      // NOT phone-verified: Bulletin writes are signed in-core with the
      // allowance key granted at login and never reach the wallet (measured
      // with the phone off: every phone-dependent operation timed out while
      // Bulletin writes succeeded). Approving here is FINAL, and the prompt
      // must say so instead of promising a checkpoint that never comes.
      return {
        title: "Publish data to the Bulletin chain",
        details: [
          `size: ${String(review.value.size)} bytes (content not decodable here)`,
        ],
        phoneVerifies: false,
        phoneNote:
          "Approving here is final. The write is signed locally with the " +
          "Bulletin allowance you approved at login and goes to the chain " +
          "immediately.",
      };
    case "AccountAlias":
      return {
        title: "Derive a contextual alias",
        details: [`product: ${review.value.callingProductId}`],
        phoneVerifies: false,
      };
    case "CreateProof":
      return {
        title: "Create a ring-VRF proof",
        details: [`product: ${review.value.callingProductId}`],
        phoneVerifies: false,
      };
    case "IdentityDisclosure":
      return {
        title: "Disclose your primary identity to a product",
        details: [`product: ${review.value.productId}`],
        phoneVerifies: false,
      };
    case "AccountAccess":
      return {
        title: "Allow one product to access another product's account",
        details: [
          `requesting: ${review.value.requestingProductId}`,
          `target: ${review.value.targetProductId}`,
        ],
        phoneVerifies: false,
      };
  }
}
