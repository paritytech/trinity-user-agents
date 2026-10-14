package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.getOrEmpty
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstanceId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toRecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toRingCollectionId
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRecyclerUpdate
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RecyclerFungibilityCalculator
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.RingCapacityProvider
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionIdWithIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.model.RingStatus
import io.paritytech.polkadotapp.feature_members_api.data.model.includedOrNull
import io.paritytech.polkadotapp.feature_members_api.data.model.includesKey
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject
import kotlin.collections.component1
import kotlin.collections.component2

private typealias VoucherKey = Pair<RingCollectionId, BandersnatchPublicKey>
private typealias VoucherPositions = Map<VoucherKey, RingPosition?>
private typealias RingStatuses = Map<RingCollectionIdWithIndex, RingStatus?>
private typealias UnloadedCounts = Map<RecyclerKey, Int>

class VoucherLocationService @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val voucherRepository: VoucherRepository,
    private val membersRepository: MembersRepository,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
    private val ringCapacityProvider: RingCapacityProvider,
    private val timeProvider: TimeProvider,
) {
    context(scope: ComputationalScope)
    fun start() {
        val chainId = chainAssetProvider.chainId()

        subscribeLocatedVouchers(chainId)
            .flatMapLatest { located ->
                // Both reads are keyed by the same rings, so they are combined rather than nested: a change
                // in how full a ring is must not tear down the unloaded-count subscription, or the other way
                // round.
                combine(
                    subscribeRingStatusesFor(chainId, located.positions),
                    subscribeUnloadedCountsFor(chainId, located),
                ) { statuses, unloadedCounts -> RingReadings(located, statuses, unloadedCounts) }
            }
            .onEach { readings -> voucherRepository.updateRecyclerState(readings.resolveRecyclerState(chainId)) }
            .launchIn(scope)
    }

    private fun subscribeLocatedVouchers(chainId: ChainId): Flow<LocatedVouchers> {
        // Every voucher, not just the ones outside a recycler: a ring keeps filling after a voucher lands
        // in it, and the member count is what the strategies read to decide when it may be spent.
        return voucherRepository.subscribeAllVouchers()
            .filter { it.isNotEmpty() }
            .distinctUntilChangedBy { vouchers ->
                vouchers.mapToSet { it.ringVrfPublicKey to (it.location as? RecyclerVoucher.Location.InRecycler)?.enteredAt }
            }
            .flatMapLatest { vouchers ->
                val instanceId = coinageInstanceIdProvider.instanceId()
                    .getOrElse { return@flatMapLatest flowOf(Result.failure(it)) }
                val keys = vouchers.map { it.recyclerValue.toRingCollectionId(instanceId) to it.ringVrfPublicKey }

                membersRepository.subscribeMembers(
                    chainId = chainId,
                    keys = keys,
                    consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
                )
                    .map { result -> result.map { positions -> LocatedVouchers(instanceId, positions) } }
            }
            .map { it.logFailure("Can't fetch location for vouchers").getOrNull() }
            .filterNotNull()
    }

    private fun subscribeRingStatusesFor(
        chainId: ChainId,
        positions: VoucherPositions,
    ): Flow<RingStatuses> {
        val ringKeys = positions.distinctRingStatusKeys()

        return membersRepository.subscribeRingStatuses(chainId, ringKeys)
            .map { it.logFailure("Can't fetch ring statuses for voucher locations").getOrEmpty() }
    }

    /**
     * Null on a failed read, never an empty map.
     *
     * The repository fills in a zero for every key the runtime holds no entry for, so within a *successful*
     * response an absent recycler genuinely has nothing unloaded. A failure says nothing at all, and
     * treating it as zero would report every ring as untouched — inflating the privacy on screen and, worse,
     * freezing an inflated maximum if the failure happened to land on a voucher's first tick in a ring.
     */
    private fun subscribeUnloadedCountsFor(
        chainId: ChainId,
        located: LocatedVouchers,
    ): Flow<UnloadedCounts?> {
        // Deduplicated per ring: the positions are per voucher, and many vouchers share one recycler.
        val recyclerKeys = located.positions.distinctRingStatusKeys().map { it.toRecyclerKey() }

        return voucherRepository.subscribeUnloadedCounts(chainId, located.instanceId, recyclerKeys)
            .map { it.logFailure("Can't fetch unloaded counts for recyclers").getOrNull() }
    }

    private fun VoucherPositions.distinctRingStatusKeys(): List<RingCollectionIdWithIndex> {
        return entries
            .mapNotNull { (voucherKey, ringPosition) ->
                val included = ringPosition?.includedOrNull() ?: return@mapNotNull null
                val ringCollectionId = voucherKey.first
                ringCollectionId to included.ringIndex
            }
            .distinct()
    }

    private suspend fun RingReadings.resolveRecyclerState(
        chainId: ChainId
    ): Map<BandersnatchPublicKey, VoucherRecyclerUpdate> {
        // Nothing can be judged included without it, so a failed read writes nothing at all — unlike a
        // failed fungibility read, which still lets the location through.
        val keysPerPage = membersRepository.getRingKeysPageSize(chainId)
            .logFailure("Can't fetch ring keys page size for voucher locations")
            .getOrNull()
            ?: return emptyMap()

        val vouchers = resolveVouchersInRecycler(keysPerPage)
        if (vouchers.isEmpty()) return emptyMap()

        val capacities = ringCapacityProvider.capacitiesFor(vouchers.mapToSet { it.recyclerKey.exponent })
            .logFailure("Can't fetch ring capacities for voucher fungibility")
            .getOrEmpty()

        return vouchers.associate { voucher ->
            val capacity = capacities[voucher.recyclerKey.exponent]
            val unloaded = unloadedCounts?.get(voucher.recyclerKey) ?: 0
            val included = voucher.location.recyclerMembers

            // Both inputs or neither: a fungibility computed from a guessed capacity or a guessed unloaded
            // count would be a number the user reads as privacy they do not have. Null leaves whatever is
            // stored standing, and the next tick with both reads in hand replaces it.
            val readable = capacity != null && unloadedCounts != null

            voucher.publicKey to VoucherRecyclerUpdate(
                location = voucher.location,
                recyclerFungibility = capacity.takeIf { readable }?.let {
                    RecyclerFungibilityCalculator.fungibility(it, included, unloaded)
                },
                maxRecyclerFungibility = capacity.takeIf { readable }?.let {
                    RecyclerFungibilityCalculator.maxFungibility(it, included, unloaded)
                },
            )
        }
    }

    private fun RingReadings.resolveVouchersInRecycler(keysPerPage: Int): List<VoucherInRecycler> {
        return located.positions.mapNotNull { (voucherKey, voucherPosition) ->
            val position = voucherPosition?.includedOrNull() ?: return@mapNotNull null
            val ringStatusKey = voucherKey.first to position.ringIndex
            val ringStatus = ringStatuses[ringStatusKey] ?: return@mapNotNull null

            if (!ringStatus.includesKey(position, keysPerPage)) return@mapNotNull null

            VoucherInRecycler(
                publicKey = voucherKey.second,
                recyclerKey = ringStatusKey.toRecyclerKey(),
                // included, not total: a proof only verifies against the keys baked into the ring root, so
                // that is the set this voucher actually hides in.
                location = RecyclerVoucher.Location.InRecycler(
                    recyclerIndex = position.ringIndex,
                    recyclerMembers = ringStatus.included,
                    enteredAt = timeProvider.now()
                ),
            )
        }
    }

    /** Positions carry the instance id along because the coinage-pallet reads are keyed by it directly. */
    private class LocatedVouchers(
        val instanceId: CoinageInstanceId,
        val positions: VoucherPositions,
    )

    private class RingReadings(
        val located: LocatedVouchers,
        val ringStatuses: RingStatuses,
        /** Null when the unloaded-count read failed — see [subscribeUnloadedCountsFor]. */
        val unloadedCounts: UnloadedCounts?,
    )

    private class VoucherInRecycler(
        val publicKey: BandersnatchPublicKey,
        val recyclerKey: RecyclerKey,
        val location: RecyclerVoucher.Location.InRecycler,
    )
}
