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
 *  a dev build can see a slow bank payment through), crypto's few minutes.
 *  The host counts a session as taking longer once this long passed since
 *  the provider's latest report, so each must outlast the longest of the
 *  rail's {@link STAGES} plus the on-chain leg, and does with the defaults. */
export const ETA_SECS = {
  Card: 300n,
  Bank: BigInt(number(env.FUNDTEST_BANK_ETA_SECS, 172_800)),
  Crypto: 600n,
} as const;

/** The simulated waits of an inbound session, milliseconds, each named by
 *  the report it comes before. The on-chain leg between them (the faucet
 *  drip, the swap, the claim) takes its own, real, time on top. */
export interface InboundStages {
  /** Card only: 3-D Secure and the authorisation, before the `Details`
   *  naming the authorised charge. */
  authorise: number;
  /** The payment reaching the provider, before `PaymentReceived` unfinalized:
   *  the capture on card, the transfer arriving on bank, the demo deposit
   *  being made on crypto. */
  receive: number;
  /** The payment becoming final, before `PaymentReceived` finalized: the
   *  charge settling on card, the transfer clearing on bank, and on crypto
   *  each of `confirmations` block confirmations. */
  confirm: number;
  /** Before `Converting`. */
  convert: number;
  /** Before the top-up and its `Crediting`. */
  credit: number;
  /** Before `Delivered`, once the credit is final. */
  deliver: number;
}

/** The simulated waits of an outbound session after its release,
 *  milliseconds. */
export interface OutboundStages {
  /** Preparing the payout, before `Converting`. */
  convert: number;
  /** The payout in transit, before `Payout`, and before each
   *  `PayoutRetrying` of a payout retried on purpose. */
  send: number;
}

/** Each rail's waits, read from `FUNDTEST_<RAIL>_<IN|OUT>_<STAGE>_MS`. The
 *  defaults follow the order of a real provider's waits, compressed so a
 *  session runs in under a minute on a simulator: bank is the slowest, crypto
 *  waits on confirmations, card is quick. */
export const STAGES: {
  In: Record<"Card" | "Bank" | "Crypto", InboundStages>;
  Out: Record<"Card" | "Bank" | "Crypto", OutboundStages>;
} = {
  In: {
    Card: {
      authorise: number(env.FUNDTEST_CARD_IN_AUTHORISE_MS, 4_000),
      receive: number(env.FUNDTEST_CARD_IN_RECEIVE_MS, 3_000),
      confirm: number(env.FUNDTEST_CARD_IN_CONFIRM_MS, 4_000),
      convert: number(env.FUNDTEST_CARD_IN_CONVERT_MS, 3_000),
      credit: number(env.FUNDTEST_CARD_IN_CREDIT_MS, 3_000),
      deliver: number(env.FUNDTEST_CARD_IN_DELIVER_MS, 2_000),
    },
    Bank: {
      authorise: 0,
      receive: number(env.FUNDTEST_BANK_IN_RECEIVE_MS, 30_000),
      confirm: number(env.FUNDTEST_BANK_IN_CONFIRM_MS, 8_000),
      convert: number(env.FUNDTEST_BANK_IN_CONVERT_MS, 4_000),
      credit: number(env.FUNDTEST_BANK_IN_CREDIT_MS, 3_000),
      deliver: number(env.FUNDTEST_BANK_IN_DELIVER_MS, 2_000),
    },
    Crypto: {
      authorise: 0,
      receive: number(env.FUNDTEST_CRYPTO_IN_RECEIVE_MS, 20_000),
      confirm: number(env.FUNDTEST_CRYPTO_IN_CONFIRM_MS, 4_000),
      convert: number(env.FUNDTEST_CRYPTO_IN_CONVERT_MS, 3_000),
      credit: number(env.FUNDTEST_CRYPTO_IN_CREDIT_MS, 3_000),
      deliver: number(env.FUNDTEST_CRYPTO_IN_DELIVER_MS, 2_000),
    },
  },
  Out: {
    Card: {
      convert: number(env.FUNDTEST_CARD_OUT_CONVERT_MS, 4_000),
      send: number(env.FUNDTEST_CARD_OUT_SEND_MS, 8_000),
    },
    Bank: {
      convert: number(env.FUNDTEST_BANK_OUT_CONVERT_MS, 6_000),
      send: number(env.FUNDTEST_BANK_OUT_SEND_MS, 25_000),
    },
    Crypto: {
      convert: number(env.FUNDTEST_CRYPTO_OUT_CONVERT_MS, 3_000),
      send: number(env.FUNDTEST_CRYPTO_OUT_SEND_MS, 10_000),
    },
  },
};

/** Build-time configuration, read from `FUNDTEST_*` variables. */
export const CONFIG = {
  /** Whether a crypto deposit is made from a faucet drip after a short wait,
   *  so a session completes without a wallet. A real transfer is taken either
   *  way. */
  demoCrypto: String(env.FUNDTEST_DEMO_CRYPTO ?? "1") !== "0",
  /** Block confirmations a crypto deposit waits for, each `confirm` apart,
   *  before it counts as final. At least one. */
  cryptoConfirmations: Math.max(
    1,
    Math.floor(number(env.FUNDTEST_CRYPTO_IN_CONFIRMATIONS, 3)),
  ),
  /** First wait before a report the host failed to take is made again; each
   *  later wait doubles, up to 30 seconds. */
  reportRetryMs: number(env.FUNDTEST_REPORT_RETRY_MS, 1_000),
} as const;
