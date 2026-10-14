package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolver
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.UnloadTokenPeriodCalculator
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.UnloadTokenResolverFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.createForCollection
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Whether the free unload allowance has run low enough to stop recycling for privacy's sake.
 *
 * Counting the allowance costs a chain query per hundred counters, and the recycling verdict is recomputed
 * every few seconds, so the count is cached: decremented as unloads happen, re-read when the period rolls
 * over or every [UNLOADS_BEFORE_REFRESH] unloads so the running total cannot drift far from the chain.
 */
@Singleton
class UnloadQuotaTracker @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val resolverFactory: UnloadTokenResolverFactory,
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase,
    private val periodCalculator: UnloadTokenPeriodCalculator,
) {
    private val mutex = Mutex()
    private var cached: FreeUnloadTokenResolver.UnloadQuota? = null
    private var unloadsSinceRefresh = 0

    suspend fun isQuotaRunningLow(): Result<Boolean> = quota().map { it.remaining <= it.limit * QUOTA_RESERVE }

    /** Called when unload tokens are claimed, so the cached count follows without another walk of the range. */
    suspend fun noteUnloadsHappened(count: Int) = mutex.withLock {
        unloadsSinceRefresh += count
        cached = cached?.let { it.copy(remaining = (it.remaining - count).coerceAtLeast(0)) }
    }

    private suspend fun quota(): Result<FreeUnloadTokenResolver.UnloadQuota> = mutex.withLock {
        cached?.takeIf { it.isFresh() }?.let { return Result.success(it) }

        val collection = activePeopleCollectionUseCase.getActivePeopleCollection()

        resolverFactory.createForCollection(collection)
            .estimateRemainingUnloadQuota(chainAssetProvider.chainId())
            .onSuccess {
                cached = it
                unloadsSinceRefresh = 0
            }
    }

    private fun FreeUnloadTokenResolver.UnloadQuota.isFresh(): Boolean =
        unloadsSinceRefresh < UNLOADS_BEFORE_REFRESH && periodCalculator.currentPeriod(periodDuration) == period

    private companion object {
        val QUOTA_RESERVE: Fraction = 20.percents

        const val UNLOADS_BEFORE_REFRESH = 5
    }
}

private operator fun Long.times(fraction: Fraction): Long = (this * fraction.fraction.toDouble()).toLong()
