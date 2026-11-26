@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.resourcesCalls
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.setStatementStoreAccount
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.CollectionSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotHolder
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotTag
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlots
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.canBeEvictedBy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.findFreeSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.hasSlotFor
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.takenCount
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.takenSlots
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
    private val currentPeriodProvider: CurrentPeriodProvider,
    private val slotReplacer: StatementStoreSlotReplacer,
) : StatementStoreSlotAllocator {
    context(diagnostics: StalenessReportCollector)
    override suspend fun allocate(
        holder: StatementStoreSlotHolder,
        strategy: OnExistingAllocationStrategy,
        priority: SlotPriority,
        tag: StatementStoreSlotTag?,
    ): Result<Unit> = diagnostics.markRegion(RCommon.string.statement_store_stall_allocating_voucher) {
        Timber.i("allocate starting; strategy=$strategy, priority=$priority, tag=${tag?.value}")

        resolveAllocateContext()
            .flatMap { context ->
                Timber.d("Resolved context: period=${context.period}, collections=${context.availableCollections}")

                renewalLock.withLock {
                    renewIfStale(context, holder.accountId).flatMap {
                        runAllocate(context, holder, strategy, priority, tag)
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

    override fun currentPeriod(): UInt = currentPeriodProvider.current()

    override suspend fun hasCurrentAllocation(target: AccountId): Result<Boolean> = runCatching {
        val period = currentPeriodProvider.current()
        allocationRepository.hasRenewedFor(knownChains.people, target, period)
    }

    override suspend fun allocatedTargets(tag: StatementStoreSlotTag): Result<Set<AccountId>> = runCatching {
        allocationRepository.accountsWithTag(knownChains.people, tag)
    }

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
        // Every stale row (not just target's) must reclaim its seq before target may take a free one,
        // otherwise a fresh allocation right after a period change can squeeze out an existing Critical slot.
        val hasStale = allocationRepository.hasAnyStale(
            chainId = context.chain.id,
            currentPeriod = context.period,
        )
        if (!hasStale) return Result.success(Unit)

        Timber.i("allocate: stale rows present; running renewer with priorityAccount=target")
        return diagnostics.markRegion(RCommon.string.statement_store_stall_renewing_vouchers) {
            renewer.renew(context, priorityAccount = target)
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun runAllocate(
        context: AllocateContext,
        holder: StatementStoreSlotHolder,
        strategy: OnExistingAllocationStrategy,
        priority: SlotPriority,
        tag: StatementStoreSlotTag?,
    ): Result<Unit> {
        return loadSlots(context).flatMap { slots ->
            Timber.i("allocate: ${slots.takenCount()}/${slots.totalCount()} seqs already claimed in period ${context.period}")

            if (strategy == OnExistingAllocationStrategy.IGNORE && slots.hasSlotFor(holder.accountId)) {
                Timber.i("allocate: target already has an active slot; strategy=IGNORE — skipping")
                Result.success(Unit)
            } else {
                claimSlot(context, slots, holder.accountId, priority)
                    .mapCatching { pick -> recordAllocation(context, pick, holder, priority, tag) }
            }
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun claimSlot(
        context: AllocateContext,
        slots: StatementStoreSlots,
        target: AccountId,
        callerPriority: SlotPriority,
    ): Result<SlotPick> {
        slots.findFreeSlot()?.let { free ->
            val pick = SlotPick(seq = free.slot.seq, collection = free.collection, evictedAccount = null)
            return submitWithProof(context, pick, target).map { pick }
        }

        return evictionCandidates(context, slots, target, callerPriority)
            .flatMap { candidates -> evict(context, candidates, target) }
            .map { evicted -> SlotPick(seq = evicted.slot.seq, collection = evicted.collection, evictedAccount = evicted.slot.accountId) }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun evict(
        context: AllocateContext,
        candidates: List<CollectionSlot<StatementStoreSlot.Taken>>,
        target: AccountId,
    ): Result<CollectionSlot<StatementStoreSlot.Taken>> {
        if (candidates.isEmpty()) {
            return Result.failure(StatementStoreSlotAllocationError.NoAllocationAvailable(IllegalStateException("No free slot and no evictable slot")))
        }

        return statementStoreSlotRepository.supportsSlotReplacement().flatMap { supportsReplacement ->
            Timber.i("allocate: no free seq; ${candidates.size} eviction candidates, supportsReplacement=$supportsReplacement")
            if (supportsReplacement) {
                slotReplacer.replaceFirstAvailable(context, candidates, target)
            } else {
                evictWithProof(context, candidates, target)
            }
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun evictWithProof(
        context: AllocateContext,
        candidates: List<CollectionSlot<StatementStoreSlot.Taken>>,
        target: AccountId,
    ): Result<CollectionSlot<StatementStoreSlot.Taken>> {
        return statementStoreSlotRepository.replacementCooldown().flatMap { cooldown ->
            val now = currentTimeContext.currentTime()
            val victim = candidates.firstOrNull { now - it.slot.since >= cooldown }

            if (victim == null) {
                Result.failure(EvictionNotYetAllowedException("No evictable slot is past the replacement cooldown $cooldown"))
            } else {
                val pick = SlotPick(seq = victim.slot.seq, collection = victim.collection, evictedAccount = victim.slot.accountId)
                submitWithProof(context, pick, target).map { victim }
            }
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun submitWithProof(
        context: AllocateContext,
        pick: SlotPick,
        target: AccountId,
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
    }

    private suspend fun recordAllocation(
        context: AllocateContext,
        pick: SlotPick,
        holder: StatementStoreSlotHolder,
        priority: SlotPriority,
        tag: StatementStoreSlotTag?,
    ) {
        pick.evictedAccount?.let { evicted ->
            allocationRepository.deleteSlot(context.chain.id, pick.collection, evicted, pick.seq)
        }
        allocationRepository.insert(
            chainId = context.chain.id,
            collection = pick.collection,
            holder = holder,
            seq = pick.seq,
            latestRenewedPeriod = context.period,
            since = currentTimeContext.currentTime(),
            priority = priority,
            tag = tag,
        )
        Timber.i("allocate: extrinsic executed for seq=${pick.seq} in ${pick.collection}; DB delta applied")
    }

    private suspend fun evictionCandidates(
        context: AllocateContext,
        slots: StatementStoreSlots,
        target: AccountId,
        callerPriority: SlotPriority,
    ): Result<List<CollectionSlot<StatementStoreSlot.Taken>>> = runCancellableCatching {
        val takenByOthers = slots.takenSlots().filter { it.slot.accountId != target }

        val priorityByAccount = allocationRepository.maxPriorityLevelsFor(
            context.chain.id,
            takenByOthers.mapToSet { it.slot.accountId },
        )

        takenByOthers
            .map { it to priorityByAccount.getPriorityOrNormal(it.slot.accountId) }
            .filter { (_, effective) -> effective.canBeEvictedBy(callerPriority) }
            .sortedWith(evictionComparator())
            .map { (candidate, _) -> candidate }
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
