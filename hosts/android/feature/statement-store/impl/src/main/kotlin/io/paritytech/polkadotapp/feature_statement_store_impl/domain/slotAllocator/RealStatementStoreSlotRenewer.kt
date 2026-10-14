@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.resourcesCalls
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.setStatementStoreAccount
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.freeSlots
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRecord
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.ExperimentalTime

class RealStatementStoreSlotRenewer @Inject constructor(
    private val allocationRepository: StatementStoreSlotAllocationRepository,
    private val slotLoader: StatementStoreSlotLoader,
    private val statementStoreOrigins: StatementStoreOrigins,
    private val extrinsicService: ExtrinsicService,
    private val currentTimeContext: CurrentTimeContext,
) : StatementStoreSlotRenewer {
    override suspend fun renew(context: AllocateContext, priorityAccount: AccountId?): Result<Unit> {
        return planRenewal(context, priorityAccount)
            .flatMap { plan ->
                submitBatch(context, plan.pairings).mapCatching { perExtrinsicResults ->
                    reconcilePairings(context, plan.pairings, perExtrinsicResults)
                    plan.overflow.forEach { allocationRepository.deleteById(it.id) }
                }
            }
            .coerceToUnit()
            .onFailure { Timber.e(it, "renewer failed (period=${context.period})") }
    }

    private suspend fun planRenewal(context: AllocateContext, priorityAccount: AccountId?): Result<RenewalPlan> {
        val stale = allocationRepository.staleRows(context.chain.id, context.period)
        if (stale.isEmpty()) {
            Timber.i("renewer: no stale rows for period=${context.period}; skipping")
            return Result.success(RenewalPlan(emptyList(), emptyList()))
        }

        return slotLoader.loadSlots(context).mapCatching { slots ->
            val sorted = stale.sortedWith(renewalComparator(priorityAccount))

            // Free seqs pooled across all available collections; a row may migrate into any of
            // them, since renewal re-claims a seq authorized by the target collection's alias.
            val freeSeqs = slots.freeSlots()

            val pairings = sorted.zip(freeSeqs) { row, free ->
                Pairing(row, free.collection, free.slot.seq)
            }
            val overflow = sorted.drop(pairings.size)

            Timber.i(
                """
                renewer: stale=${stale.size}, freeSeqs=${freeSeqs.size},
                renewing=${pairings.size}, overflowDrop=${overflow.size},
                priorityAccount=$priorityAccount
                """.trimIndent()
            )

            RenewalPlan(pairings, overflow)
        }
    }

    private suspend fun submitBatch(
        context: AllocateContext,
        pairings: List<Pairing>,
    ): Result<List<Result<ExtrinsicStatus.InBlock>>> {
        if (pairings.isEmpty()) return Result.success(emptyList())

        return extrinsicService.submitExtrinsicsAndAwaitInBlock(context.chain) {
            pairings.forEach { pairing ->
                val origin = statementStoreOrigins.asResourcesStatementStoreSlot(
                    period = context.period,
                    seq = pairing.seq,
                    collection = pairing.collection,
                ).getOrThrow()
                extrinsic(origin = origin) {
                    resourcesCalls.setStatementStoreAccount(context.period, pairing.seq, pairing.row.accountId)
                }
            }
        }
    }

    private suspend fun reconcilePairings(
        context: AllocateContext,
        pairings: List<Pairing>,
        perExtrinsicResults: List<Result<ExtrinsicStatus.InBlock>>,
    ) {
        pairings.forEachIndexed { index, pairing ->
            val result = perExtrinsicResults[index]
            if (result.isSuccess) {
                allocationRepository.markRenewed(
                    id = pairing.row.id,
                    period = context.period,
                    since = currentTimeContext.currentTime(),
                    newSeq = pairing.seq,
                    newCollection = pairing.collection,
                )
            } else {
                Timber.w(result.exceptionOrNull(), "renewer: extrinsic failed for row=${pairing.row.id}; deleting")
                allocationRepository.deleteById(pairing.row.id)
            }
        }
    }
}

private class Pairing(
    val row: StatementStoreSlotAllocationRecord,
    val collection: PeopleCollection,
    val seq: UInt,
)

private class RenewalPlan(
    val pairings: List<Pairing>,
    val overflow: List<StatementStoreSlotAllocationRecord>,
)

internal fun renewalComparator(priorityAccount: AccountId?): Comparator<StatementStoreSlotAllocationRecord> {
    return compareByDescending<StatementStoreSlotAllocationRecord> { it.priority.level }
        .thenBy { record -> if (priorityAccount != null && record.accountId == priorityAccount) 0 else 1 }
        .thenByDescending { it.since }
}
