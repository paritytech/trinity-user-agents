package io.paritytech.polkadotapp.feature_transactions.api.data

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

interface TransactionExtensionVersions {
    /**
     * The chains the app builds as general (v5) transactions, each with its transaction extension version.
     * Every other chain is v4. Whether a given runtime can verify a signed general transaction is left to the caller.
     */
    suspend fun forGeneralTransactionChains(): Result<Map<ChainId, Byte>>
}
