import { createClient, createTransport } from "@parity/truapi";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import { ProofRefused, type ProofSource } from "./nova/turn.js";

/**
 * TrUAPI `account` calls made by the host as one product, for the personhood
 * proof that buys relay credentials.
 *
 * The runtime makes a provider for `productId`, so the core sees that product
 * as the caller and applies every check it applies to the product itself:
 * the key owner's and the context product's manifest grants, and the user's
 * account-access decision. The host asks; it decides nothing.
 */

/** How long the core may take to answer one call. A proof waits on the chain for its ring. */
const REQUEST_TIMEOUT_MS = 60_000;

/** The error tag a refusal carries: the domain error's, or the call error's own. */
export function refused(error: unknown): ProofRefused {
  const value = error as {
    tag?: string;
    value?: {
      value?: { tag?: string; value?: { reason?: string } };
      reason?: string;
    };
  };
  if (value?.tag === "Domain") {
    const domain = value.value?.value;
    return new ProofRefused(domain?.tag ?? "Unknown", domain?.value?.reason);
  }
  return new ProofRefused(value?.tag ?? "Unknown", value?.value?.reason);
}

export async function accountProofs(
  runtime: Pick<WorkerPairingHostRuntime, "createProvider">,
  productId: string,
): Promise<{ source: ProofSource; dispose(): void }> {
  const provider = await runtime.createProvider({
    productId,
    executionKind: "App",
  });
  const transport = createTransport(provider, {
    requestTimeoutMs: REQUEST_TIMEOUT_MS,
  });
  const account = createClient(transport).account;
  return {
    source: {
      listRingVrfKeys: async (owner) => {
        const result = await account.listRingVrfKeys({
          owner,
          disclosure: "Anonymized",
        });
        if (result.isErr()) throw refused(result.error);
        return result.value;
      },
      createAccountProof: async (request) => {
        const result = await account.createAccountProof(request);
        if (result.isErr()) throw refused(result.error);
        return result.value;
      },
    },
    dispose: () => {
      transport.dispose();
      provider.dispose();
    },
  };
}
