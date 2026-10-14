package io.paritytech.polkadotapp.feature_transaction_storage_api.domain

import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface TransactionStorageService {
    /**
     * Submits [data] as a transaction-storage `store` extrinsic on the Bulletin chain
     * signed by [origin]. Returns the blake2b-256 hash of [data] as a 0x-prefixed hex string.
     */
    suspend fun store(data: ByteArray, origin: TransactionOrigin): Result<String>
}
