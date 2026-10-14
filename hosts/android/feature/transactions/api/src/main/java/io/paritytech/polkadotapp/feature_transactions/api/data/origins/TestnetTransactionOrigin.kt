package io.paritytech.polkadotapp.feature_transactions.api.data.origins

import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SignedTransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

/**
 * Origins useful on Substrate testnets
 */
interface TestnetTransactionOrigins {
    /**
     * Origin based on well-known Alice account. Alice is usually the testnet account that is seeded with funds
     * and also have super-user capabilities (can call sudo.sudo)
     */
    fun alice(): TransactionOrigin

    fun fundingOrigin(): SignedTransactionOrigin
}
