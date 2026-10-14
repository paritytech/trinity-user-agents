package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.signer.origins

import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface TransactionStorageOrigins {
    suspend fun asResourcesLongTermStorage(
        period: UInt,
        counter: UByte,
        collection: PeopleCollection,
    ): Result<TransactionOrigin>
}
