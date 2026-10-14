package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.data.memory.AccumulatingMapCache
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toRingCollectionId
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject
import javax.inject.Singleton

/**
 * How many keys a recycler's ring holds, per denomination.
 *
 * Cached for the process: it is chain configuration — including the coinage instance the collection ids are
 * derived from — so it cannot change under a running app, and the balance would otherwise re-read it on every
 * recomputation.
 */
@Singleton
class RingCapacityProvider @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val membersRepository: MembersRepository,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) {
    private val capacities = AccumulatingMapCache<ValueExponent, Int> { denominations ->
        coinageInstanceIdProvider.instanceId().flatMap { instanceId ->
            membersRepository.getCollections(
                chainId = chainAssetProvider.chainId(),
                collectionIds = denominations.map { it.toRingCollectionId(instanceId) },
                consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
            ).map { collections ->
                denominations.mapNotNull { denomination ->
                    val collection = collections[denomination.toRingCollectionId(instanceId)]
                        ?: return@mapNotNull null

                    denomination to collection.ringSize.ringCapacity
                }.toMap()
            }
        }
    }

    /**
     * Denominations that could not be read are left out rather than failing the caller, which reads them as
     * rings that are never full — so a strategy waiting on anonymity keeps waiting instead of releasing a
     * voucher on a capacity we do not have.
     */
    suspend fun capacitiesFor(denominations: Set<ValueExponent>): Result<Map<ValueExponent, Int>> {
        return capacities.get(denominations)
    }

    /**
     * Returns already cached values of [capacitiesFor]. null in case there isn't sufficient cache data to cover [denominations]
     */
    suspend fun peekCapacitiesFor(denominations: Set<ValueExponent>): Map<ValueExponent, Int>? {
        return capacities.peek(denominations)
    }
}
