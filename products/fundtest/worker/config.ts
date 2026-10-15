// The provider's terms and its build-time configuration.

import manifest from "../worker-manifest.json";

/** Decimals of the user's payment balance, CASH. */
export const BALANCE_DECIMALS = 6;
/** One CASH in balance units. */
export const ONE_CASH = 1_000_000n;

/** Smallest and largest amount the provider takes, in balance units. */
export const MIN_AMOUNT = 10n * ONE_CASH;
export const MAX_AMOUNT = 5_000n * ONE_CASH;

/** EUR cents one CASH is worth to the provider. */
export const EUR_CENTS_PER_CASH = 99n;
/** The provider's fee, in basis points of what moves. */
export const PROVIDER_FEE_BPS = 100n;
/** How long a quote holds, in milliseconds. */
export const QUOTE_TTL_MS = 60_000;

/** A fiat currency the card and bank routes take. */
export const FIAT = { symbol: "EUR", decimals: 2 } as const;

/** The network a crypto route moves on, as the manifest names it. A crypto
 *  ask for any other network is refused. */
export const CRYPTO_NETWORK = "polkadot";

/** A crypto asset the provider takes or pays out on Asset Hub. `cashPerUnit`
 *  is the deterministic demo rate, in balance units per whole unit of the
 *  asset; `networkFee` is in the asset's smallest unit. */
export interface CryptoAsset {
  symbol: "PAS" | "USDT" | "USDC" | "CASH";
  decimals: number;
  cashPerUnit: bigint;
  networkFee: bigint;
}

export const CRYPTO_ASSETS: Record<CryptoAsset["symbol"], CryptoAsset> = {
  PAS: {
    symbol: "PAS",
    decimals: 10,
    cashPerUnit: 4n * ONE_CASH,
    networkFee: 100_000_000n,
  },
  USDT: {
    symbol: "USDT",
    decimals: 6,
    cashPerUnit: ONE_CASH,
    networkFee: 20_000n,
  },
  USDC: {
    symbol: "USDC",
    decimals: 6,
    cashPerUnit: ONE_CASH,
    networkFee: 20_000n,
  },
  CASH: {
    symbol: "CASH",
    decimals: 6,
    cashPerUnit: ONE_CASH,
    networkFee: 1_000n,
  },
};

/** The fixed network fee on a fiat route, in EUR cents. */
export const FIAT_NETWORK_FEE_CENTS = 10n;

/** The countries each route serves, as the Worker manifest declares them. */
export const ROUTE_COUNTRIES: Partial<
  Record<"CARD" | "BANK" | "CRYPTO", readonly string[]>
> = Object.fromEntries(
  manifest.includes.funding.routes.flatMap((route) =>
    "countries" in route ? [[route.mode, route.countries]] : [],
  ),
);

/** The account the bank rail tells the user to pay. A documentation IBAN:
 *  nothing is ever sent there. */
export const BANK_DETAILS = {
  beneficiary: "Fundtest Reference Provider",
  account: "DE89 3704 0044 0532 0130 00",
  bankCode: "COBADEFFXXX",
} as const;

/** How long a crypto deposit address takes payment. */
export const DEPOSIT_WINDOW_MS = 30 * 60_000;

const env = import.meta.env ?? {};
const number = (value: string | undefined, fallback: number) => {
  const parsed = Number(value);
  return value !== undefined && value !== "" && Number.isFinite(parsed)
    ? parsed
    : fallback;
};

/** Expected seconds until the funds arrive, by rail: the upper end of card's
 *  one to five minutes, bank's one to two days (`FUNDTEST_BANK_ETA_SECS`, so
 *  a dev build can see a slow bank payment through), crypto's few minutes. */
export const ETA_SECS = {
  Card: 300n,
  Bank: BigInt(number(env.FUNDTEST_BANK_ETA_SECS, 172_800)),
  Crypto: 600n,
} as const;

/** Build-time configuration, read from `FUNDTEST_*` variables. */
export const CONFIG = {
  /** Whether a crypto deposit is made from a faucet drip after a short wait,
   *  so a session completes without a wallet. A real transfer is taken either
   *  way. */
  demoCrypto: String(env.FUNDTEST_DEMO_CRYPTO ?? "1") !== "0",
  /** Wait before a card payment arrives, milliseconds. */
  cardArrivalMs: number(env.FUNDTEST_CARD_ARRIVAL_MS, 5_000),
  /** Wait after "I've sent funds" before a bank transfer arrives. */
  bankArrivalMs: number(env.FUNDTEST_BANK_ARRIVAL_MS, 30_000),
  /** Wait before a demo crypto deposit is made. */
  cryptoArrivalMs: number(env.FUNDTEST_CRYPTO_ARRIVAL_MS, 20_000),
  /** Wait between the user's payment completing and the payout arriving. */
  payoutMs: number(env.FUNDTEST_PAYOUT_MS, 15_000),
  /** First wait before a report the host failed to take is made again; each
   *  later wait doubles, up to 30 seconds. */
  reportRetryMs: number(env.FUNDTEST_REPORT_RETRY_MS, 1_000),
} as const;
