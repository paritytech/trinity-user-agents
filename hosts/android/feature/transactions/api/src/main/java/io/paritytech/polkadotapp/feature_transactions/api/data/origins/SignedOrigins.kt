package io.paritytech.polkadotapp.feature_transactions.api.data.origins

import io.paritytech.polkadotapp.common.domain.model.EncodedPrivateKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SignedTransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource

interface SignedOrigins {
    suspend fun wallet(): TransactionOrigin

    suspend fun candidate(): TransactionOrigin

    fun signedOrigin(metaAccount: MetaAccount): TransactionOrigin

    /**
     * Build a signed origin from a raw 32-byte Sr25519 secret seed. Used for RFC-0006 top-ups
     * whose source is an arbitrary private key rather than a local account.
     */
    fun signedTransactionSourceSr25519PrivateKey(privateKey: EncodedPrivateKey): Result<TransactionSignerSource.Signed>
}

fun SignedOrigins.signedOriginSr25519PrivateKey(privateKey: EncodedPrivateKey): Result<TransactionOrigin> {
    return signedTransactionSourceSr25519PrivateKey(privateKey).map(::SignedTransactionOrigin)
}
