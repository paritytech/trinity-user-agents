package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount

interface TransactionSignInteractor {
    val account: SigningAccount

    suspend fun parseSigningContent(): Result<ParsedSigningContent>

    suspend fun humanReadableRepresentation(): Result<String>

    suspend fun sign(): Result<SignedTransaction>
}
