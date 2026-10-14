package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.BalanceEvaluationMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherBuckets
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyCoins
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.preClassifyVouchers
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageAssetsUseCase
import kotlinx.coroutines.flow.first
import javax.inject.Inject

/**
 * Which funds an operation may draw on.
 *
 * Stated at every call site rather than defaulted, because the two answers differ in what the user has
 * agreed to — and picking the wrong one silently spends privacy they were buying.
 */
enum class SpendScope {
    /** What can be spent outright. */
    SPENDABLE,

    /**
     * Also the funds the strategy is holding back for privacy. Only for an operation the user has confirmed,
     * and only where the strategy permits the offer at all — see [SpendScope] handling in the selector.
     */
    WITH_CONFIRMATION
}

/**
 * What an operation may spend right now.
 *
 * Separate from [CoinageAssetsUseCase] because the answer needs the current recycling verdicts, and the
 * evaluator that produces those reads the asset join itself — folding this back in would make the two
 * depend on each other.
 */
class CoinageAssetSelector @Inject constructor(
    private val coinageAssetsUseCase: CoinageAssetsUseCase,
    private val strategyProvider: RecyclingStrategyProvider,
    private val settings: CoinageRecyclingStrategySettings,
    private val evaluator: CoinRecyclingEvaluator,
    private val voucherUsabilityContextFactory: VoucherUsabilityContextFactory,
) {
    /** Suspends until the first verdict exists, so nothing is spent before the strategy has judged it. */
    suspend fun getSelectableCoinsByScope(): Map<SpendScope, List<Coin>> {
        val verdicts = evaluator.verdicts.first()
        val minted = coinageAssetsUseCase.getCoins().preClassifyCoins().minted
        val offerAllowed = confirmedSpendAllowed()

        return SpendScope.entries.associateWith { scope ->
            val allowed = allowedStates(scope, offerAllowed)

            minted.filter { verdicts[it.derivationIndex] in allowed }
        }
    }

    suspend fun getSelectableVouchersByScope(): Map<SpendScope, List<RecyclerVoucher>> {
        val buckets = voucherBuckets()
        val offerAllowed = confirmedSpendAllowed()

        return SpendScope.entries.associateWith { scope ->
            if (scope.widens(offerAllowed)) buckets.usable + buckets.gainingPrivacy else buckets.usable
        }
    }

    suspend fun getSelectableCoins(scope: SpendScope): List<Coin> = getSelectableCoinsByScope().getValue(scope)

    suspend fun getSelectableVouchers(scope: SpendScope): List<RecyclerVoucher> =
        getSelectableVouchersByScope().getValue(scope)

    suspend fun getVouchersGainingPrivacy(): List<RecyclerVoucher> = voucherBuckets().gainingPrivacy

    /**
     * Every coin that can still be loaded into a recycler, whatever the strategy thinks of it — including one past
     * the age it may be spent at.
     */
    suspend fun getRecyclableCoins(): List<Coin> = coinageAssetsUseCase.getCoins().preClassifyCoins().minted

    /** Every voucher the chain would accept, whatever the strategy is holding back. Should only be used for planning after making sure used has acknowledged the privacy risks */
    suspend fun getOnChainSpendableVouchers(): List<RecyclerVoucher> {
        val buckets = voucherBuckets()

        return buckets.usable + buckets.gainingPrivacy
    }

    private fun allowedStates(scope: SpendScope, offerAllowed: Boolean): Set<CoinRecyclingState> = when {
        scope.widens(offerAllowed) -> setOf(CoinRecyclingState.ALLOW_USE, CoinRecyclingState.TO_RECYCLE)
        else -> setOf(CoinRecyclingState.ALLOW_USE)
    }

    private fun SpendScope.widens(offerAllowed: Boolean) = this == SpendScope.WITH_CONFIRMATION && offerAllowed

    private suspend fun confirmedSpendAllowed(): Boolean = currentStrategy().allowsConfirmedSpend()

    private suspend fun currentStrategy() = strategyProvider.voucherStrategyFor(settings.getStrategy())

    private suspend fun voucherBuckets(): VoucherBuckets {
        val tracked = coinageAssetsUseCase.getVouchers()

        val denominations = tracked.mapToSet { it.voucher.recyclerValue }
        val usability = voucherUsabilityContextFactory.create(BalanceEvaluationMode.COMPLETE, denominations)

        return tracked.preClassifyVouchers(currentStrategy(), usability)
    }
}
