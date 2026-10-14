package io.paritytech.polkadotapp.feature_pgas_impl.data.signer.origins

import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface PgasOrigins {
    suspend fun asPgasClaim(period: UInt, slotIndex: UInt, collection: PeopleCollection): Result<TransactionOrigin>
}
