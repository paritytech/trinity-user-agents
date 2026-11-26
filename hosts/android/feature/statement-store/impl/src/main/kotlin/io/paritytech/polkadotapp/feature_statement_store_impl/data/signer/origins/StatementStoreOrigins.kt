package io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface StatementStoreOrigins {
    suspend fun asResourcesStatementStoreSlot(
        period: UInt,
        seq: UInt,
        collection: PeopleCollection,
    ): Result<TransactionOrigin>

    suspend fun asResourcesNotificationSlot(
        period: UInt,
        seq: UByte,
        collection: PeopleCollection,
    ): Result<TransactionOrigin>

    fun asResourcesSlotHolder(holderKeypair: Sr25519Keypair): TransactionOrigin
}
