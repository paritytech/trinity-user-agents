package io.paritytech.polkadotapp.feature_statement_store_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.model.StmtStoreAllowanceEntry
import kotlin.time.Duration

interface StatementStoreSlotRepository {
    suspend fun maxSlotsPerPeriod(collection: PeopleCollection): Result<UInt>

    /** Min delay before an alias's entry can be replaced. */
    suspend fun replacementCooldown(): Result<Duration>

    suspend fun allowanceEntries(
        chainId: ChainId,
        period: UInt,
        candidates: Collection<BandersnatchAlias>,
    ): Map<BandersnatchAlias, StmtStoreAllowanceEntry>
}
