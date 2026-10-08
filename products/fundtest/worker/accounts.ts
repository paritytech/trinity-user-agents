// The accounts the provider signs with: one session account per session,
// derived from host entropy under this provider's product as getcash derives
// its burners (packages/ephemeral), and the faucet that stands in for the
// fiat leg.

import {
  deriveKeypairWithSecret,
  deriveSeedKeypair,
  toSchnorrkelSecret,
  type EphemeralKeypair,
  type EphemeralKeypairWithSecret,
} from "@getsome/ephemeral";
import { mnemonicToEntropy } from "@polkadot-labs/hdkd-helpers";
import { blake2b } from "@noble/hashes/blake2.js";
import { CONFIG } from "./config";
import { onTestnet } from "./chain";
import { fromHex, toHex, truapi, unwrap } from "./host";

/** The entropy label of a session's account, in getcash's `onramp:eph:`
 *  form. The host derives the same seed for the same user, product and label,
 *  so a restarted worker finds the same account. */
const sessionLabel = (intent: string) => `onramp:eph:fundtest:${intent}`;

/** The session account for `intent`. Its secret is never stored. */
export async function sessionAccount(
  intent: string,
): Promise<EphemeralKeypairWithSecret> {
  const client = await truapi();
  const label = new TextEncoder().encode(sessionLabel(intent));
  const { entropy } = await unwrap(
    client.entropy.derive({ context: toHex(label) }),
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

/** Whether the build carries a faucet seed of a shape it can read. */
export function faucetConfigured(): boolean {
  const raw = CONFIG.faucetSeed;
  const words = raw.split(/\s+/).length;
  return /^0x[0-9a-fA-F]{64}$/.test(raw) || words === 12 || words === 24;
}

/** The faucet account. Refused off the testnet, since its seed ships inside
 *  the worker bundle. */
export function faucetAccount(): EphemeralKeypair {
  if (!onTestnet())
    throw new Error("the faucet runs only on the Paseo Next v2 testnet");
  if (!faucetConfigured()) {
    throw new Error(
      "no faucet: build with FUNDTEST_FAUCET_SEED set to a mnemonic or 0x entropy",
    );
  }
  const raw = CONFIG.faucetSeed;
  const entropy = raw.startsWith("0x")
    ? fromHex(raw)
    : mnemonicToEntropy(raw.replace(/\s+/g, " "));
  return deriveSeedKeypair(entropy);
}

/** The account a user's outbound payment is collected into: the faucet, so
 *  the CASH stays with the account that funds the inbound sessions, or this
 *  user's own provider account when the build has no faucet. */
export async function collectionAccount(): Promise<`0x${string}`> {
  if (faucetConfigured() && onTestnet())
    return toHex(faucetAccount().publicKey);
  return toHex((await sessionAccount("collection")).publicKey);
}
