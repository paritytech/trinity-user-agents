// TODO(development_createAccountProof): dev-only escape hatch, yet to be
// removed before a production release. Everything for it lives in this file,
// its test and one re-export in `index.ts`; delete those to remove it.
import type {
  HostAccountCreateProofRequest,
  HostAccountCreateProofResponse,
  ProductProofContext,
  TrUApiClient,
  VersionedHostAccountCreateProofError,
} from "./generated/index.js";
import type { ResultAsync } from "./generated/client.js";
import type { CallErrorValue, HexString } from "./scale.js";

/** `productId` the signing host reads as "use the suffix bytes verbatim". */
const RAW_PROOF_CONTEXT_PRODUCT_ID = "raw:";

/** Same as `HostAccountCreateProofRequest`, with the 32-byte context given raw. */
export interface DevelopmentCreateProofRequest extends Omit<
  HostAccountCreateProofRequest,
  "context"
> {
  /** The exact 32 bytes the proof is bound to, as `0x`-prefixed hex. */
  context: HexString;
}

/**
 * `account.createAccountProof` with a verbatim 32-byte proof context instead of
 * a product-namespaced one.
 *
 */
export function development_createAccountProof(
  client: Pick<TrUApiClient, "account">,
  request: DevelopmentCreateProofRequest,
): ResultAsync<
  HostAccountCreateProofResponse,
  CallErrorValue<VersionedHostAccountCreateProofError>
> {
  const { context, ...rest } = request;
  return client.account.createAccountProof({
    ...rest,
    context: rawProofContext(context),
  });
}

function rawProofContext(context: HexString): ProductProofContext {
  const digits = context.startsWith("0x") ? context.slice(2) : null;
  if (
    digits === null ||
    digits.length !== 64 ||
    !/^[0-9a-fA-F]*$/.test(digits)
  ) {
    throw new TypeError(
      `development_createAccountProof: context must be 32 bytes of 0x-prefixed hex, got ${JSON.stringify(context)}`,
    );
  }
  return {
    productId: RAW_PROOF_CONTEXT_PRODUCT_ID,
    suffix: { tag: "Raw", value: context },
  };
}
