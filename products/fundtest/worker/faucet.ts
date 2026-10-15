// The public Paseo faucet, which stands in for the money a card or bank
// payment would bring.
//
// `POST /drip/web` takes either a captcha token or a signed message. The
// signed form needs no browser: the recipient signs `faucet:<address>:<ms>`
// with its own key (sr25519, raw or `<Bytes>`-wrapped), and the faucet checks
// the signature against the address and that the timestamp is under five
// minutes old (polkadot-testnet-faucet, src/dripper/signatureVerify.ts). It
// sends 5000 PAS from Paseo Asset Hub, teleported to `parachain_id` when that
// names another chain, at most once per 20 hours per recipient. The answer is
// NDJSON: status lines, then `{ hash }` or `{ error }`, always with HTTP 200
// once the stream started.
//
// The worker reaches it with its own `fetch`, which the host gates per domain
// behind the product's `Remote` permission (shared/faucet.ts).

import { FAUCET_URL } from "../shared/faucet";
import { toHex } from "../shared/hex";

/** What one drip sends, in PAS's smallest unit (10 decimals). The teleport
 *  to Asset Hub Next takes a small fee out of it on arrival. */
export const DRIP_AMOUNT = 5_000n * 10_000_000_000n;

/** What a drip is allowed to lose on the way, so a deposit up to
 *  `DRIP_AMOUNT - DRIP_MARGIN` is covered by one drip. */
export const DRIP_MARGIN = 10n * 10_000_000_000n;

/** The account a drip goes to and the signature it proves itself with. */
export interface DripRecipient {
  address: string;
  signBytes: (data: Uint8Array) => Promise<Uint8Array>;
}

/** Sends the request body and resolves with the response text. */
export type FaucetTransport = (url: string, body: string) => Promise<string>;

/** The faucet refused the drip. `quota` is its once-per-20-hours refusal. */
export class FaucetRefused extends Error {
  constructor(
    message: string,
    readonly quota: boolean,
  ) {
    super(`the faucet refused the drip: ${message}`);
    this.name = "FaucetRefused";
  }
}

/** The message the recipient signs. */
export const dripMessage = (address: string, now: number) =>
  `faucet:${address}:${now}`;

/** The JSON body of a signed drip to `parachainId`. */
export async function dripBody(
  recipient: DripRecipient,
  parachainId: number,
  now: number,
): Promise<string> {
  const message = dripMessage(recipient.address, now);
  const signature = toHex(
    await recipient.signBytes(new TextEncoder().encode(message)),
  );
  return JSON.stringify({
    address: recipient.address,
    parachain_id: String(parachainId),
    signature,
    message,
  });
}

/** The extrinsic hash a drip answered with, or a thrown `FaucetRefused`. */
export function parseDripResponse(text: string): `0x${string}` {
  let last: { hash?: string; error?: string } | undefined;
  for (const line of text.split("\n")) {
    if (!line.trim()) continue;
    try {
      last = JSON.parse(line) as typeof last;
    } catch {
      throw new FaucetRefused(
        `unreadable answer: ${line.slice(0, 120)}`,
        false,
      );
    }
  }
  if (!last) throw new FaucetRefused("empty answer", false);
  if (last.error !== undefined) {
    throw new FaucetRefused(last.error, /quota|once per day/i.test(last.error));
  }
  if (typeof last.hash !== "string" || !last.hash.startsWith("0x")) {
    throw new FaucetRefused("no transaction hash in the answer", false);
  }
  return last.hash as `0x${string}`;
}

/** The transport a worker uses: the runtime's own `fetch`. */
export const fetchTransport: FaucetTransport = async (url, body) => {
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/x-ndjson",
    },
    body,
  });
  const text = await response.text();
  if (!response.ok) {
    // Errors before the stream starts come back as one JSON object.
    try {
      const { error } = JSON.parse(text) as { error?: string };
      throw new FaucetRefused(error ?? `HTTP ${response.status}`, false);
    } catch (error) {
      if (error instanceof FaucetRefused) throw error;
      throw new Error(`faucet HTTP ${response.status}: ${text.slice(0, 120)}`);
    }
  }
  return text;
};

/** Asks the faucet to drip PAS to `recipient` on parachain `parachainId`
 *  (Asset Hub Next, 1500, for this provider). Resolves with the faucet's
 *  extrinsic hash on Paseo Asset Hub once it is in a block; the teleport
 *  lands on the parachain a few blocks later. */
export async function requestDrip(
  recipient: DripRecipient,
  parachainId: number,
  transport: FaucetTransport = fetchTransport,
  now: () => number = Date.now,
): Promise<`0x${string}`> {
  const body = await dripBody(recipient, parachainId, now());
  return parseDripResponse(await transport(FAUCET_URL, body));
}
