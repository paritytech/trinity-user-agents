package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.hasEverBeenOnChain
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.isInRecycler
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedCoin
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.isMinting
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.isOnboarding

/**
 * The split every consumer of coinage assets starts from, before any strategy verdict is applied.
 *
 * It exists so the recycling evaluator and the balance can share one reading of what the user holds while
 * running at different rates — the evaluator on a throttled tick, the balance on every change. Sharing the
 * *output* instead would make the balance as stale as the tick, which is what a balance may never be.
 */
data class CoinBuckets(
    /** On chain with a known age: the only coins a strategy may gate. */
    val minted: List<Coin>,
    /** Not on chain yet, but nothing has proven the transaction minting them never ran. */
    val minting: List<Coin>,
) {
    /**
     * Everything that counts as the user's money. Deliberately not "every free coin": a coin whose minting
     * transaction failed is free of any claim yet will never arrive, and counting it would put balance on
     * screen that can never be spent.
     */
    val total: List<Coin> = minted + minting
}

data class VoucherBuckets(
    val usable: List<RecyclerVoucher>,
    /** In a recycler, not yet released by the current strategy. */
    val gainingPrivacy: List<RecyclerVoucher>,
    /** On their way into a ring, or not yet on chain at all. */
    val minting: List<RecyclerVoucher>,
) {
    /** As with [CoinBuckets.total], vouchers whose minting failed are left out rather than counted. */
    val total: List<RecyclerVoucher> = usable + gainingPrivacy + minting
}

fun List<TrackedCoin>.preClassifyCoins(): CoinBuckets = CoinBuckets(
    minted = filter { it.isMinted() }.coins(),
    minting = filter { it.isMinting() }.coins(),
)

fun List<TrackedVoucher>.preClassifyVouchers(
    strategy: CoinRecyclingStrategy,
    context: VoucherUsabilityContext,
): VoucherBuckets {
    val free = filter { it.state.isFree }
    val (usable, rest) = free.partition { strategy.isVoucherUsable(it.voucher, context) }

    return VoucherBuckets(
        usable = usable.vouchers(),
        gainingPrivacy = rest.filter { it.voucher.isInRecycler() }.vouchers(),
        minting = rest.filter { it.isOnboarding() || it.isMinting() }.vouchers(),
    )
}

/** Settled: on chain, with an age the chain has told us — the only coins a strategy may gate. */
fun TrackedCoin.isMinted(): Boolean = state.isFree && coin.isOnChain && coin.hasEverBeenOnChain

fun List<TrackedCoin>.coins(): List<Coin> = map(TrackedCoin::coin)

fun List<TrackedVoucher>.vouchers(): List<RecyclerVoucher> = map(TrackedVoucher::voucher)
