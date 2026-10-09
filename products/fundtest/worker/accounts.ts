// The accounts the provider signs with: one session account per session and
// one reserve account per user, derived from host entropy under this
// provider's product as getcash derives its burners (packages/ephemeral).

import {
  deriveKeypairWithSecret,
  toSchnorrkelSecret,
  type EphemeralKeypairWithSecret,
} from "@getsome/ephemeral";
import { blake2b } from "@noble/hashes/blake2.js";
import { fromHex, toHex, truapi, unwrap } from "./host";

/** The entropy label of a session's account, in getcash's `onramp:eph:`
 *  form. The host derives the same seed for the same user, product and label,
 *  so a restarted worker finds the same account. */
const sessionLabel = (intent: string) => `onramp:eph:fundtest:${intent}`;

/** The context `entropy.derive` takes for `label`: its 32-byte hash, since
 *  the host accepts at most 32 bytes and a session label is longer. */
export const entropyContext = (label: string) =>
  toHex(blake2b(new TextEncoder().encode(label), { dkLen: 32 }));

/** The session account for `intent`. Its secret is never stored. */
export async function sessionAccount(
  intent: string,
): Promise<EphemeralKeypairWithSecret> {
  const client = await truapi();
  const { entropy } = await unwrap(
    client.entropy.derive({ context: entropyContext(sessionLabel(intent)) }),
    "entropy.derive",
  );
  return deriveKeypairWithSecret(fromHex(entropy));
}

/** The session account's secret in the layout `payment.topUp` takes. */
export const topUpSecret = (account: EphemeralKeypairWithSecret) =>
  toHex(toSchnorrkelSecret(account.secretKey));

/** The id top-up attempt `attempt` of a session account is made under: its
 *  public key for the first, a hash of the key and the attempt after, as
 *  getcash's worker names its claims (worker/src/topup-id.js). */
export function topUpIdFor(
  publicKey: Uint8Array,
  attempt: number,
): `0x${string}` {
  if (attempt === 0) return toHex(publicKey);
  const input = new Uint8Array(publicKey.length + 4);
  input.set(publicKey);
  new DataView(input.buffer).setUint32(publicKey.length, attempt, true);
  return toHex(blake2b(input, { dkLen: 32 }));
}

/** The provider's own account for this user: where an outbound payment is
 *  collected and where a session sweeps the PAS a faucet drip left over. */
export const reserveAccount = () => sessionAccount("collection");
