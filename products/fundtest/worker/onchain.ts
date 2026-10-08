// The real leg: getcash's route choice, deposit sizing and conversion
// pipeline, driven for one session account at a time.
//
// A session account receives the deposit on Asset Hub. One transaction signed
// by it converts the deposit to CASH (none for CASH, the PSM for USDT, the
// pool for PAS and for a stable the PSM will not take) and sends the CASH to
// the same account on People by teleport or reserve transfer, whichever the
// chains trust. The CASH there is what `payment.topUp` claims.

import {
  DEFAULT_INCLUSION_TIMEOUT_MS,
  DEFAULT_KEEP_NATIVE_FOR_FEES,
  DEFAULT_REMOTE_FEE_BUFFER,
  DEFAULT_TICK_TIMEOUT_MS,
  DIRECT_SLIPPAGE_PCT,
  PASEO_ASSET_HUB_PARA_ID,
  PASEO_PEOPLE_PARA_ID,
  PASEO_UNDERLYING_ASSET_ID,
  STABLE_TOKENS,
  chooseCashTransfer,
  chooseRoute,
  depositTokenOf,
  discoverPools,
  freshTickState,
  recordedRoute,
  sizeNativeBudget,
  tickOnce,
  type ConversionRoute,
  type DepositAsset,
  type FundingStep,
} from "@getsome/funding";
import { TOKENS, type TokenSpec } from "@getsome/core";
import type { EphemeralKeypair } from "@getsome/ephemeral";
import { CASH_SETTLEMENT, createPeopleChainPort } from "@getsome/people";
import {
  estimateDotUsdFundingSizing,
  estimateFundingSizing,
  estimatePsmFundingSizing,
  estimateStableFundingSizing,
} from "@getcash/lib/funding-fees";
import {
  MultiAddress,
  paseo_next_v2,
  paseo_people_next,
} from "@polkadot-api/descriptors";
import type { PolkadotClient } from "polkadot-api";
import { bounded, signOptionsFor, type Chains } from "./chain";
import { toHex } from "./host";
import type { SavedPlan, SavedRoute, SavedTick } from "./saved";

/** Claims are multiples of 0.01 CASH, the coinage instance's asset unit. */
export const CLAIM_UNIT = 10_000n;

/** A crypto asset the user may deposit, as the provider's manifest names it. */
export type CryptoSymbol = "PAS" | "USDT" | "USDC" | "CASH";

const DEPOSIT_OF: Record<CryptoSymbol, DepositAsset> = {
  PAS: "native",
  USDT: "USDT",
  USDC: "USDC",
  CASH: "dotUSD",
};

/** The token a plan's session account receives. */
export function depositToken(plan: SavedPlan): TokenSpec {
  return depositTokenOf(recordedRoute(plan.route));
}

/** The symbol the user sees for a token: CASH, not Paseo's dotUSD. */
export const displaySymbol = (token: TokenSpec) =>
  token.assetHubId === TOKENS.CASH.assetHubId ? "CASH" : token.symbol;

const asSaved = (route: ConversionRoute): SavedRoute =>
  route.tier === "dotusd"
    ? { tier: "dotusd" }
    : route.tier === "psm"
      ? { tier: "psm", external: route.external, feeRate: route.feeRate }
      : route.external === undefined
        ? { tier: "pool" }
        : { tier: "pool", external: route.external };

/**
 * Chooses the route and sizes the deposit that lands `settleAmount` of CASH on
 * People. `asset` is what the user deposits; a fiat session names none and
 * takes the PSM's external when the PSM can serve, the native otherwise, as
 * getcash's fiat rails do.
 */
export async function planConversion(
  chains: Chains,
  asset: CryptoSymbol | undefined,
  settleAmount: bigint,
  probeAddress: string,
): Promise<SavedPlan> {
  const api = chains.assetHub.getTypedApi(paseo_next_v2);
  const route = await bounded(
    chooseRoute(api, {
      direction: "mint",
      internalAmount: settleAmount,
      ...(asset === undefined ? {} : { deposit: DEPOSIT_OF[asset] }),
    }),
    DEFAULT_TICK_TIMEOUT_MS,
    "route choice",
  );
  const sizing = {
    ahClient: chains.assetHub,
    peopleClient: chains.people,
    underlyingAssetId: PASEO_UNDERLYING_ASSET_ID,
    peopleParaId: PASEO_PEOPLE_PARA_ID,
    settleAmount,
    probeAddress,
  };
  const base = {
    route: asSaved(route),
    depositSymbol: depositTokenOf(route).symbol,
    keepNativeForFees: DEFAULT_KEEP_NATIVE_FOR_FEES.toString(),
    slippagePct: DIRECT_SLIPPAGE_PCT,
  };
  if (route.tier === "psm") {
    const fees = await estimatePsmFundingSizing({ ...sizing, route });
    return {
      ...base,
      deposit: fees.quotedDeposit.toString(),
      quotedDeposit: fees.quotedDeposit.toString(),
      remoteFeeBuffer: fees.remoteFeeBuffer.toString(),
    };
  }
  if (route.tier === "dotusd") {
    const fees = await estimateDotUsdFundingSizing(sizing);
    return {
      ...base,
      deposit: fees.quotedDeposit.toString(),
      quotedDeposit: fees.quotedDeposit.toString(),
      remoteFeeBuffer: fees.remoteFeeBuffer.toString(),
    };
  }
  if (route.external !== undefined) {
    const fees = await estimateStableFundingSizing({
      ...sizing,
      route: { tier: "pool", external: route.external },
      slippagePct: DIRECT_SLIPPAGE_PCT,
    });
    return {
      ...base,
      deposit: fees.askedDeposit.toString(),
      quotedDeposit: fees.quotedDeposit.toString(),
      remoteFeeBuffer: fees.remoteFeeBuffer.toString(),
    };
  }
  // The native pool tier falls back to getcash's static fee figures when the
  // live estimate fails.
  const fees = await estimateFundingSizing(sizing);
  const remoteFeeBuffer = fees?.remoteFeeBuffer ?? DEFAULT_REMOTE_FEE_BUFFER;
  const keepNativeForFees =
    fees?.keepNativeForFees ?? DEFAULT_KEEP_NATIVE_FOR_FEES;
  const deposit = await sizeNativeBudget({
    client: chains.assetHub,
    underlyingAssetId: PASEO_UNDERLYING_ASSET_ID,
    settleAmount,
    remoteFeeBuffer,
    keepNativeForFees,
    slippagePct: DIRECT_SLIPPAGE_PCT,
  });
  return {
    ...base,
    deposit: deposit.toString(),
    remoteFeeBuffer: remoteFeeBuffer.toString(),
    keepNativeForFees: keepNativeForFees.toString(),
  };
}

/** What `address` holds of `token` on Asset Hub, at the best or the
 *  finalized block. */
export async function depositBalance(
  assetHub: PolkadotClient,
  token: TokenSpec,
  address: string,
  at: "best" | "finalized",
): Promise<bigint> {
  const api = assetHub.getTypedApi(paseo_next_v2);
  if (token.assetHubId === undefined) {
    const account = await api.query.System.Account.getValue(address, { at });
    return account.data.free;
  }
  const held = await api.query.Assets.Account.getValue(
    token.assetHubId,
    address,
    { at },
  );
  return held?.balance ?? 0n;
}

/**
 * Sends `amount` of `token` from `from` to `address` on Asset Hub, raised to
 * the asset's `min_balance` so the transfer creates the account, as getcash's
 * demo faucet does (lib/faucet.ts). Resolves once it is finalized.
 */
export async function transferDeposit(
  assetHub: PolkadotClient,
  from: EphemeralKeypair,
  token: TokenSpec,
  address: string,
  amount: bigint,
): Promise<void> {
  const api = assetHub.getTypedApi(paseo_next_v2);
  const dest = MultiAddress.Id(address);
  const tx =
    token.assetHubId === undefined
      ? api.tx.Balances.transfer_keep_alive({ dest, value: amount })
      : await (async () => {
          const asset = await api.query.Assets.Asset.getValue(
            token.assetHubId!,
          );
          const minBalance = asset?.min_balance ?? 0n;
          return api.tx.Assets.transfer_keep_alive({
            id: token.assetHubId!,
            target: dest,
            amount: amount < minBalance ? minBalance : amount,
          });
        })();
  const result = await tx.signAndSubmit(from.signer);
  if (!result.ok) {
    throw new Error(
      `the faucet's ${token.symbol} transfer failed on chain; is the faucet funded?`,
    );
  }
}

const restoreTick = (saved: SavedTick | undefined) => {
  const state = freshTickState();
  if (!saved) return state;
  return {
    ...state,
    attempts: saved.attempts,
    psmRefusals: saved.psmRefusals,
    xcmSubmitted: saved.xcmSubmitted,
    peopleAtXcm: BigInt(saved.peopleAtXcm),
    fundsSeenAt: saved.fundsSeenAt,
    nonceAtSubmit: saved.nonceAtSubmit,
    inclusionBlock: saved.inclusionBlock,
  };
};

const persistTick = (state: ReturnType<typeof freshTickState>): SavedTick => ({
  attempts: state.attempts,
  psmRefusals: state.psmRefusals,
  xcmSubmitted: state.xcmSubmitted,
  peopleAtXcm: state.peopleAtXcm.toString(),
  fundsSeenAt: state.fundsSeenAt,
  nonceAtSubmit: state.nonceAtSubmit,
  inclusionBlock: state.inclusionBlock,
});

/**
 * One tick of getcash's pipeline for the session account, as its worker runs
 * one (worker/src/engine.js): restore the tick state, act at most once, hand
 * the state back. `persist` runs before a submit leaves and after the tick.
 * Throws getcash's `FundingShortfallError` and `FundingHeldError` when the
 * run cannot finish; anything else is the next tick's to retry.
 */
export async function convertOnce(args: {
  chains: Chains;
  account: EphemeralKeypair;
  plan: SavedPlan;
  settleAmount: bigint;
  tick: SavedTick | undefined;
  persist: (tick: SavedTick) => Promise<void>;
}): Promise<FundingStep> {
  const { chains, account, plan } = args;
  const route = recordedRoute(plan.route);
  const api = chains.assetHub.getTypedApi(paseo_next_v2);
  const [pool, stablePool] =
    route.tier === "pool"
      ? await bounded(
          discoverPools(
            api,
            route.external === undefined
              ? [PASEO_UNDERLYING_ASSET_ID]
              : [
                  PASEO_UNDERLYING_ASSET_ID,
                  STABLE_TOKENS[route.external].assetHubId,
                ],
          ),
          DEFAULT_TICK_TIMEOUT_MS,
          "pool discovery",
        )
      : [];
  const transfer = await bounded(
    chooseCashTransfer({
      assetHub: chains.assetHub,
      people: chains.people,
      assetHubParaId: PASEO_ASSET_HUB_PARA_ID,
      peopleParaId: PASEO_PEOPLE_PARA_ID,
    }),
    DEFAULT_TICK_TIMEOUT_MS,
    "cash transfer choice",
  );
  const peoplePort = createPeopleChainPort({ client: chains.people });
  const state = restoreTick(args.tick);
  try {
    const outcome = await tickOnce(
      {
        api,
        peopleApi: chains.people.getTypedApi(paseo_people_next),
        route,
        transfer,
        pool,
        stablePool,
        address: account.address,
        signer: account.signer,
        beneficiaryHex: toHex(account.publicKey),
        settleAmount: args.settleAmount,
        peopleParaId: PASEO_PEOPLE_PARA_ID,
        assetHubParaId: PASEO_ASSET_HUB_PARA_ID,
        remoteFeeBuffer: BigInt(plan.remoteFeeBuffer),
        keepNativeForFees: BigInt(plan.keepNativeForFees),
        slippagePct: plan.slippagePct,
        ...(plan.quotedDeposit === undefined
          ? {}
          : { quotedDeposit: BigInt(plan.quotedDeposit) }),
        tickTimeoutMs: DEFAULT_TICK_TIMEOUT_MS,
        inclusionTimeoutMs: DEFAULT_INCLUSION_TIMEOUT_MS,
        signOptions: await signOptionsFor(chains.assetHub),
        readFinalizedUnderlyingOnPeople: (ss58) =>
          peoplePort.settlementBalance(ss58, CASH_SETTLEMENT, {
            at: "finalized",
          }),
        now: Date.now,
        onBeforeSubmit: () => args.persist(persistTick(state)),
      },
      state,
    );
    return outcome.step;
  } finally {
    await args.persist(persistTick(state));
  }
}

/** The CASH the session account holds on People in the finalized block,
 *  floored to the claim unit. */
export async function claimable(
  people: PolkadotClient,
  address: string,
): Promise<bigint> {
  const held = await createPeopleChainPort({
    client: people,
  }).settlementBalance(address, CASH_SETTLEMENT, { at: "finalized" });
  return (held / CLAIM_UNIT) * CLAIM_UNIT;
}
