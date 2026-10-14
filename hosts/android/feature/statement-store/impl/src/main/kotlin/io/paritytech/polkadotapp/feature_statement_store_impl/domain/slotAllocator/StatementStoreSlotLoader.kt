@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_people_api.domain.BandersnatchKeyResolver
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementSlotsForCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlots
import io.paritytech.polkadotapp.feature_statement_store_impl.data.extension.statementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotRepository
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class StatementStoreSlotLoader @Inject constructor(
    private val statementStoreSlotRepository: StatementStoreSlotRepository,
    private val bandersnatchKeyResolver: BandersnatchKeyResolver,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    suspend fun loadSlots(context: AllocateContext): Result<StatementStoreSlots> {
        return context.availableCollections
            .mapAsync { collection -> loadSlotsForCollection(context, collection) }
            .flattenResult()
            .map(::StatementStoreSlots)
    }

    private suspend fun loadSlotsForCollection(
        context: AllocateContext,
        collection: PeopleCollection,
    ): Result<StatementSlotsForCollection> {
        return statementStoreSlotRepository.maxSlotsPerPeriod(collection).mapCatching { maxSlots ->
            val tld = dotNsTldProvider.getTldRetrying()
            val aliasesByIndex = (0u until maxSlots).associateWith { seq ->
                val ctx = BandersnatchContext.statementStoreSlot(tld, context.period, seq)
                bandersnatchKeyResolver.getAliasInContext(collection, ctx)
            }
            val taken = statementStoreSlotRepository.allowanceEntries(context.chain.id, context.period, aliasesByIndex.values)
            val slots = aliasesByIndex.map { (seq, alias) ->
                val entry = taken[alias]
                if (entry == null) {
                    StatementStoreSlot.Free(seq)
                } else {
                    StatementStoreSlot.Taken(
                        seq = seq,
                        accountId = entry.accountId,
                        since = Instant.fromEpochSeconds(entry.since.toLong()),
                    )
                }
            }
            StatementSlotsForCollection(collection, slots)
        }
    }
}
