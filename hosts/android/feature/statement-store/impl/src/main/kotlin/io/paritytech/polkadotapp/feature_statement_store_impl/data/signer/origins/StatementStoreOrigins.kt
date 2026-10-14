package io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins

import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface StatementStoreOrigins {
    suspend fun asResourcesStatementStoreSlot(
        period: UInt,
        seq: UInt,
        collection: PeopleCollection,
    ): Result<TransactionOrigin>
}
