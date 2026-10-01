import { Reader } from "./scale-reader.js";

/** What the core promised to keep allowed, as it records it after an allocation. */
export type LedgerTarget =
  | { tag: "ProductStatementAllowance"; productId: string }
  | { tag: "WalletSso" }
  | { tag: "Account"; label: string };

/**
 * Decode the core's Statement Store renewal ledger, the SCALE bytes of
 * `Vec<TrackedStatementRenewalTarget>` kept under `StatementRenewalTargets`.
 * Throws when the bytes are not that layout.
 *
 * The core writes a `ProductStatementAllowance` entry only after it has
 * completed the on-chain allocation for that product, whether it registered a
 * new slot or found one already held. The entry carries a recipe, not a key.
 */
export function decodeRenewalLedger(bytes: Uint8Array): LedgerTarget[] {
  const reader = new Reader(bytes);
  const targets: LedgerTarget[] = [];
  for (let remaining = reader.compact(); remaining > 0; remaining -= 1) {
    const variant = reader.u8();
    if (variant === 0)
      targets.push({
        tag: "ProductStatementAllowance",
        productId: reader.text(),
      });
    else if (variant === 1) targets.push({ tag: "WalletSso" });
    else if (variant === 2) {
      reader.take(32);
      targets.push({ tag: "Account", label: reader.text() });
    } else throw new Error("an unknown ledger target");
    const owner = reader.u8();
    if (owner === 1) reader.take(32);
    else if (owner !== 0) throw new Error("an option flag is invalid");
  }
  reader.finish();
  return targets;
}

/** The product id as the core compares it: trimmed, NFC, lower case. */
export function normalizeProductId(productId: string): string {
  return productId.trim().normalize("NFC").toLowerCase();
}

/** What the ledger says about one product. */
export type AllocationRecord = "recorded" | "none" | "unreadable";

/**
 * Whether the ledger records a completed Statement Store allocation for
 * `productId`. `bytes` is the stored ledger, or undefined when none is stored.
 * This is a local record, not a chain check, and "none" does not mean the
 * chain holds no allowance.
 */
export function allocationRecord(
  bytes: Uint8Array | undefined,
  productId: string,
): AllocationRecord {
  if (bytes === undefined) return "none";
  try {
    const wanted = normalizeProductId(productId);
    return decodeRenewalLedger(bytes).some(
      (target) =>
        target.tag === "ProductStatementAllowance" &&
        target.productId === wanted,
    )
      ? "recorded"
      : "none";
  } catch {
    return "unreadable";
  }
}
