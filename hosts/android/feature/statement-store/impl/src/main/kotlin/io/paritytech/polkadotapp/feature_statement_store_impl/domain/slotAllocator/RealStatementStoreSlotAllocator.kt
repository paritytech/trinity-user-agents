@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.resourcesCalls
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.setStatementStoreAccount
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.CollectionSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlots
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.filterReplaceableSlots
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.findFreeSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.hasSlotFor
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.takenCount
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.totalCount
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import io.paritytech.polkadotapp.common.R as RCommon

class RealStatementStoreSlotAllocator @Inject constructor(
    @param:ApplicationContext private val appContext: Context,
    private val extrinsicService: ExtrinsicService,
    private val statementStoreOrigins: StatementStoreOrigins,
    private val statementStoreSlotRepository: StatementStoreSlotRepository,
    private val slotLoader: StatementStoreSlotLoader,
    private val contextResolver: AllocateContextResolver,
    private val knownChains: KnownChains,
    private val allocationRepository: StatementStoreSlotAllocationRepository,
    private val renewer: StatementStoreSlotRenewer,
    private val renewalLock: StatementStoreSlotRenewalLock,
    private val currentTimeContext: CurrentTimeContext,
) : StatementStoreSlotAllocator {
    context(diagnostics: StalenessReportCollector)
    override suspend fun allocate(
        target: AccountId,
        strategy: OnExistingAllocationStrategy,
        priority: SlotPriority,
    ): Result<Unit> = diagnostics.markRegion(RCommon.string.statement_store_stall_allocating_voucher) {
        Timber.i("allocate starting; strategy=$strategy, priority=$priority")

        resolveAllocateContext()
            .flatMap { context ->
                Timber.d("Resolved context: period=${context.period}, collections=${context.availableCollections}")

                renewalLock.withLock {
                    renewIfStale(context, target).flatMap {
                        runAllocate(context, target, strategy, priority)
                    }
                }
            }
            .onFailure { Timber.e(it, "allocate failed") }
            .mapErrorNotInstance<_, StatementStoreSlotAllocationError> { StatementStoreSlotAllocationError.Unknown(it) }
    }

    override suspend fun deallocateAllSlots(target: AccountId): Result<Unit> = runCatching {
        allocationRepository.deleteAllForAccount(knownChains.people, target)
        Timber.i("deallocateAllSlots: dropped local slot rows for target")
    }
        .onFailure { Timber.e(it, "deallocateAllSlots failed") }
        .mapErrorNotInstance<_, StatementStoreSlotAllocationError> { StatementStoreSlotAllocationError.Unknown(it) }

    override suspend fun allocationsFor(target: AccountId): Result<StatementStoreSlots> = contextResolver.resolve()
        .flatMap { slotLoader.loadSlots(it) }

    override suspend fun scheduleSlotRenewals(): Result<Unit> = runCatching {
        StatementStoreSlotRenewalWorker.schedule(appContext)
        Timber.i("scheduled periodic slot renewer")
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun resolveAllocateContext(): Result<AllocateContext> = diagnostics.markRegion(RCommon.string.stall_reading_chain_state) {
        contextResolver.resolve()
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun loadSlots(context: AllocateContext): Result<StatementStoreSlots> = diagnostics.markRegion(RCommon.string.statement_store_stall_reading_slots) {
        slotLoader.loadSlots(context)
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun renewIfStale(context: AllocateContext, target: AccountId): Result<Unit> {
        val hasStale = allocationRepository.hasStaleFor(
            chainId = context.chain.id,
            accountId = target,
            currentPeriod = context.period,
        )
        if (!hasStale) return Result.success(Unit)

        Timber.i("allocate: target has stale rows; running renewer with priorityAccount=target")
        return diagnostics.markRegion(RCommon.string.statement_store_stall_renewing_vouchers) {
            renewer.renew(context, priorityAccount = target)
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun runAllocate(
        context: AllocateContext,
        target: AccountId,
        strategy: OnExistingAllocationStrategy,
        priority: SlotPriority,
    ): Result<Unit> {
        return loadSlots(context).flatMap { slots ->
            Timber.i("allocate: ${slots.takenCount()}/${slots.totalCount()} seqs already claimed in period ${context.period}")

            if (strategy == OnExistingAllocationStrategy.IGNORE && slots.hasSlotFor(target)) {
                Timber.i("allocate: target already has an active slot; strategy=IGNORE — skipping")
                Result.success(Unit)
            } else {
                pickSeq(context, slots, target, priority)
                    .mapError { StatementStoreSlotAllocationError.NoAllocationAvailable(it) }
                    .flatMap { pick -> submitAndRecord(context, pick, target, priority) }
            }
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun submitAndRecord(
        context: AllocateContext,
        pick: SlotPick,
        target: AccountId,
        priority: SlotPriority,
    ): Result<Unit> = diagnostics.markRegion(RCommon.string.stall_submitting_transaction) {
        Timber.i("allocate: picked seq=${pick.seq} in ${pick.collection}, evictedAccount=${pick.evictedAccount}; submitting")
        statementStoreOrigins.asResourcesStatementStoreSlot(context.period, pick.seq, pick.collection)
            .flatMap { origin ->
                extrinsicService.submitExtrinsicAndAwaitExecution(context.chain, origin) {
                    resourcesCalls.setStatementStoreAccount(context.period, pick.seq, target)
                }
                    .flattenExecutionFailure()
                    .coerceToUnit()
            }
            .mapCatching {
                pick.evictedAccount?.let { evicted ->
                    allocationRepository.deleteSlot(context.chain.id, pick.collection, evicted, pick.seq)
                }
                allocationRepository.insert(
                    chainId = context.chain.id,
                    collection = pick.collection,
                    accountId = target,
                    seq = pick.seq,
                    latestRenewedPeriod = context.period,
                    since = currentTimeContext.currentTime(),
                    priority = priority,
                )
                Timber.i("allocate: extrinsic executed for seq=${pick.seq} in ${pick.collection}; DB delta applied")
            }
    }

    private suspend fun pickSeq(
        context: AllocateContext,
        slots: StatementStoreSlots,
        target: AccountId,
        callerPriority: SlotPriority,
    ): Result<SlotPick> {
        slots.findFreeSlot()?.let { free ->
            return Result.success(SlotPick(seq = free.slot.seq, collection = free.collection, evictedAccount = null))
        }

        return statementStoreSlotRepository.replacementCooldown().mapCatching { cooldown ->
            val candidates = with(currentTimeContext) { slots.filterReplaceableSlots(cooldown) }
                .filter { it.slot.accountId != target }

            val priorityByAccount = allocationRepository.maxPriorityLevelsFor(
                context.chain.id,
                candidates.mapToSet { it.slot.accountId },
            )

            val evictableWithPriority = candidates
                .map { it to priorityByAccount.getPriorityOrNormal(it.slot.accountId) }
                .filter { (_, effective) -> effective.level <= callerPriority.level }

            val victim = evictableWithPriority.minWithOrNull(evictionComparator())?.first
                ?: error("No free slot and no evictable slot (cooldown=$cooldown, callerPriority=$callerPriority)")

            Timber.i("allocate: evicting seq=${victim.slot.seq} in ${victim.collection} (since=${victim.slot.since})")
            SlotPick(seq = victim.slot.seq, collection = victim.collection, evictedAccount = victim.slot.accountId)
        }
    }

    private fun Map<AccountId, SlotPriority>.getPriorityOrNormal(accountId: AccountId): SlotPriority {
        return get(accountId) ?: SlotPriority.Normal
    }
}

private class SlotPick(val seq: UInt, val collection: PeopleCollection, val evictedAccount: AccountId?)

/**
 * Picks the cheapest victim when no free seq exists: lowest effective priority first,
 * then oldest on-chain `since` within that tier — so a `High` caller doesn't burn a
 * `High` slot when a `Normal` one is available, and within a tier we evict LRU. Candidates
 * are pooled across all available collections.
 */
private fun evictionComparator(): Comparator<Pair<CollectionSlot<StatementStoreSlot.Taken>, SlotPriority>> {
    return compareBy<Pair<CollectionSlot<StatementStoreSlot.Taken>, SlotPriority>> { it.second.level }
        .thenBy { it.first.slot.since }
}
