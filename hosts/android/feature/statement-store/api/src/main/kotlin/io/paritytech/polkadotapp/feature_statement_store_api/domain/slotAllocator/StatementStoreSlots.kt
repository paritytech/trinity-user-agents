@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import kotlin.time.Duration
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Snapshot of every seq in the current period across all available collections, each
 * collection's seqs marked as [StatementStoreSlot.Free] or [StatementStoreSlot.Taken].
 * Returned by [StatementStoreSlotAllocator.allocationsFor].
 */
@JvmInline
value class StatementStoreSlots(val perCollection: List<StatementSlotsForCollection>)

/** One collection's slice of the snapshot. */
data class StatementSlotsForCollection(
    val collection: PeopleCollection,
    val slots: List<StatementStoreSlot>,
)

sealed interface StatementStoreSlot {
    val seq: UInt

    data class Free(override val seq: UInt) : StatementStoreSlot

    data class Taken(
        override val seq: UInt,
        val accountId: AccountId,
        val since: Instant,
    ) : StatementStoreSlot
}

/** A slot together with the collection whose seq space it belongs to. */
data class CollectionSlot<out T : StatementStoreSlot>(
    val collection: PeopleCollection,
    val slot: T,
)

fun StatementStoreSlots.hasSlotFor(accountId: AccountId): Boolean {
    return perCollection.any { forCollection ->
        forCollection.slots.any { it is StatementStoreSlot.Taken && it.accountId == accountId }
    }
}

/** Free seqs pooled across all collections, each tagged with its owning collection. */
fun StatementStoreSlots.freeSlots(): List<CollectionSlot<StatementStoreSlot.Free>> {
    return perCollection.flatMap { forCollection ->
        forCollection.slots
            .filterIsInstance<StatementStoreSlot.Free>()
            .map { CollectionSlot(forCollection.collection, it) }
    }
}

fun StatementStoreSlots.findFreeSlot(): CollectionSlot<StatementStoreSlot.Free>? = freeSlots().firstOrNull()

/**
 * Taken slots across all collections whose cooldown has elapsed and may be replaced, each
 * tagged with its collection. Does not filter by the incoming account — callers add their
 * own predicates (e.g. excluding their own slots).
 */
context(currentTimeContext: CurrentTimeContext)
fun StatementStoreSlots.filterReplaceableSlots(cooldown: Duration): List<CollectionSlot<StatementStoreSlot.Taken>> {
    val now = currentTimeContext.currentTime()
    return perCollection.flatMap { forCollection ->
        forCollection.slots
            .filterIsInstance<StatementStoreSlot.Taken>()
            .filter { now - it.since >= cooldown }
            .map { CollectionSlot(forCollection.collection, it) }
    }
}

fun StatementStoreSlots.takenCount(): Int =
    perCollection.sumOf { forCollection -> forCollection.slots.count { it is StatementStoreSlot.Taken } }

fun StatementStoreSlots.totalCount(): Int = perCollection.sumOf { it.slots.size }
