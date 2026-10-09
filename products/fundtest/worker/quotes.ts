// Deterministic pricing for the host's quote asks. A quote names the terms it
// was given for in its id, since an assignment hands back only the quote and
// not the rail and asset it priced.

import type {
  FundingDirection,
  FundingQuoteAnswer,
  FundingQuoteAsk,
  FundingRail,
} from "@parity/truapi";
import {
  CRYPTO_ASSETS,
  CRYPTO_NETWORK,
  EUR_CENTS_PER_CASH,
  ETA_SECS,
  FIAT,
  FIAT_NETWORK_FEE_CENTS,
  MAX_AMOUNT,
  MIN_AMOUNT,
  ONE_CASH,
  PROVIDER_FEE_BPS,
  QUOTE_TTL_MS,
  ROUTE_COUNTRIES,
  type CryptoAsset,
} from "./config";

/** The terms a quote was given for. */
export interface QuoteTerms {
  direction: FundingDirection;
  rail: FundingRail;
  asset: string;
  /** Balance units: credited for In, debited for Out. */
  amount: bigint;
}

const QUOTE_ID_VERSION = "ft1";

/** The id a quote is given under, carrying its terms and expiry. */
export function quoteIdOf(terms: QuoteTerms, expiresAt: number): string {
  return [
    QUOTE_ID_VERSION,
    terms.direction,
    terms.rail,
    terms.asset,
    terms.amount,
    expiresAt,
  ].join(".");
}

/** The terms a quote id carries, or `undefined` for an id this provider did
 *  not give. */
export function termsOfQuoteId(quoteId: string): QuoteTerms | undefined {
  const [version, direction, rail, asset, amount] = quoteId.split(".");
  if (
    version !== QUOTE_ID_VERSION ||
    asset === undefined ||
    amount === undefined
  )
    return undefined;
  if (direction !== "In" && direction !== "Out") return undefined;
  if (rail !== "Card" && rail !== "Bank" && rail !== "Crypto") return undefined;
  if (!/^\d+$/.test(amount)) return undefined;
  return { direction, rail, asset, amount: BigInt(amount) };
}

const ceilDiv = (numerator: bigint, denominator: bigint) =>
  (numerator + denominator - 1n) / denominator;

const feeOn = (amount: bigint) => ceilDiv(amount * PROVIDER_FEE_BPS, 10_000n);

/** How many of an asset's smallest units one balance unit is worth, as the
 *  fraction `assetUnits / balanceUnits`. */
interface Rate {
  assetUnits: bigint;
  balanceUnits: bigint;
  /** The rail's fixed network fee, in the asset's smallest unit. */
  networkFee: bigint;
}

/** Smallest units of the asset `amount` balance units buy, rounded up when
 *  the user pays and down when the provider does. */
function toAsset(rate: Rate, amount: bigint, round: "up" | "down"): bigint {
  const numerator = amount * rate.assetUnits;
  return round === "up"
    ? ceilDiv(numerator, rate.balanceUnits)
    : numerator / rate.balanceUnits;
}

/** Balance units `amount` smallest units of the asset are worth, rounded up. */
function toBalance(rate: Rate, amount: bigint): bigint {
  return ceilDiv(amount * rate.balanceUnits, rate.assetUnits);
}

/** 99 euro cents per CASH. */
const FIAT_RATE: Rate = {
  assetUnits: EUR_CENTS_PER_CASH,
  balanceUnits: ONE_CASH,
  networkFee: FIAT_NETWORK_FEE_CENTS,
};

/** One whole unit of the asset per `cashPerUnit` balance units. */
const cryptoRate = (asset: CryptoAsset): Rate => ({
  assetUnits: 10n ** BigInt(asset.decimals),
  balanceUnits: asset.cashPerUnit,
  networkFee: asset.networkFee,
});

/** The rate for an asset on a rail, or `undefined` when the rail does not
 *  take it. */
function rateFor(rail: FundingRail, asset: string): Rate | undefined {
  if (rail === "Crypto") {
    return Object.hasOwn(CRYPTO_ASSETS, asset)
      ? cryptoRate(CRYPTO_ASSETS[asset as CryptoAsset["symbol"]])
      : undefined;
  }
  return asset === FIAT.symbol ? FIAT_RATE : undefined;
}

const MODE_OF_RAIL = { Card: "CARD", Bank: "BANK", Crypto: "CRYPTO" } as const;

const refused = (
  reason: Extract<FundingQuoteAnswer, { tag: "Refused" }>["value"]["reason"],
) => ({ tag: "Refused", value: { reason } }) satisfies FundingQuoteAnswer;

/** The provider's answer to an ask, priced at `now` (Unix milliseconds). */
export function answerAsk(
  ask: FundingQuoteAsk,
  now: number,
): FundingQuoteAnswer {
  const countries = ROUTE_COUNTRIES[MODE_OF_RAIL[ask.rail]];
  if (
    ask.country !== undefined &&
    countries &&
    !countries.includes(ask.country.toUpperCase())
  ) {
    return refused({ tag: "CountryUnsupported" });
  }
  if (
    ask.rail === "Crypto" &&
    ask.network !== undefined &&
    ask.network !== CRYPTO_NETWORK
  ) {
    return refused({
      tag: "Other",
      value: { message: `only the ${CRYPTO_NETWORK} network is served` },
    });
  }
  const rate = rateFor(ask.rail, ask.asset);
  if (!rate) {
    return refused({
      tag: "Other",
      value: { message: `${ask.asset} is not offered by ${ask.rail}` },
    });
  }
  if (ask.amount < MIN_AMOUNT)
    return refused({ tag: "BelowMinimum", value: { min: MIN_AMOUNT } });
  if (ask.amount > MAX_AMOUNT)
    return refused({ tag: "AboveMaximum", value: { max: MAX_AMOUNT } });

  const expiresAt = now + QUOTE_TTL_MS;
  const quoteId = quoteIdOf(ask, expiresAt);
  const common = {
    quoteId,
    etaSecs: ETA_SECS[ask.rail],
    expiresAt: BigInt(expiresAt),
  };
  if (ask.direction === "In") {
    // The user pays the asset; the fees are in it too.
    const base = toAsset(rate, ask.amount, "up");
    const providerFee = feeOn(base);
    return {
      tag: "Quoted",
      value: {
        quote: {
          ...common,
          sendAmount: base + providerFee + rate.networkFee,
          receiveAmount: ask.amount,
          providerFee,
          networkFee: rate.networkFee,
        },
      },
    };
  }
  // The user pays balance units; the fees are in them too.
  const providerFee = feeOn(ask.amount);
  const networkFee = toBalance(rate, rate.networkFee);
  const paidOut = ask.amount - providerFee - networkFee;
  if (paidOut <= 0n)
    return refused({ tag: "BelowMinimum", value: { min: MIN_AMOUNT } });
  return {
    tag: "Quoted",
    value: {
      quote: {
        ...common,
        sendAmount: ask.amount,
        receiveAmount: toAsset(rate, paidOut, "down"),
        providerFee,
        networkFee,
      },
    },
  };
}

/** What the user pays the provider for `terms`, in the asset's smallest unit,
 *  as `answerAsk` prices it. */
export function priceOf(terms: QuoteTerms): bigint | undefined {
  const rate = rateFor(terms.rail, terms.asset);
  if (!rate) return undefined;
  const base = toAsset(rate, terms.amount, "up");
  return base + feeOn(base) + rate.networkFee;
}
