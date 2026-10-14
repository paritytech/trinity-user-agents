package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts

/**
 * Decides which coins go into the recycler, and when what comes back out may be spent again.
 *
 * Both halves belong together because they are two ends of the same trade: the first spends the user's
 * balance on privacy, the second says how long it stays spent.
 */
interface CoinRecyclingStrategy {
    /**
     * Verdict for every coin in [coins], which the caller must have narrowed to coins of known age.
     *
     * Batched rather than per-coin because the budget in [snapshot] is shared — each gated coin consumes
     * part of it, so a coin's verdict depends on what earlier ones in the same batch took.
     *
     * Suspends because the limits an implementation enforces are read rather than given to it: what the
     * chain will still accept, what allowance is left. Callers pass what to decide, not what to decide with
     * — bar [mode], which is how long they can wait for those reads.
     */
    context(conversion: CoinageBalanceConversionContext)
    suspend fun evaluate(
        coins: List<Coin>,
        snapshot: RecyclingSnapshot,
        mode: BalanceEvaluationMode,
    ): RecyclingVerdicts

    fun isVoucherUsable(voucher: RecyclerVoucher, context: VoucherUsabilityContext): Boolean

    /**
     * Whether balance this strategy is holding back may still be spent once the user confirms.
     *
     * False for a strategy whose point is that it will not — offering to spend what it is hiding would undo
     * the setting the user chose.
     */
    fun allowsConfirmedSpend(): Boolean
}

/**
 * How much of the policy a caller can afford to wait for.
 *
 * The balance renders nothing until the first verdicts land, so the pass that produces them cannot be held
 * up by a chain read that a lagging node may take seconds to answer.
 */
enum class BalanceEvaluationMode {
    /** Every limit is consulted, however long the chain takes to answer. */
    COMPLETE,

    /**
     * Only the limits already in hand are applied. Whatever a slower limit would have gated stays spendable
     * until the [COMPLETE] pass that follows says otherwise, so this trades a downward correction moments
     * later for a balance the user can see at once.
     */
    IMMEDIATE,
}

/**
 * What the budget is measured against, as it stands *before* the current pass gates anything.
 *
 * [evaluate] accumulates into [unavailable] as it admits coins, so the pending balance it leaves behind is
 * the same number the ceiling was checked against — the invariant holds by construction rather than by a
 * second calculation that could disagree.
 */
data class RecyclingSnapshot(
    val total: Balance,
    val unavailable: Balance,
)
