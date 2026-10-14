package io.paritytech.polkadotapp.feature_products_impl.domain.origin

import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_impl.domain.ProductAccountDerivationUseCase
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SignedTransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import javax.inject.Inject

interface ProductAccountOrigins {
    suspend fun productAccountOrigin(accountId: ProductAccountId): Result<TransactionOrigin>
}

class RealProductAccountOrigins @Inject constructor(
    private val derivation: ProductAccountDerivationUseCase,
) : ProductAccountOrigins {
    override suspend fun productAccountOrigin(accountId: ProductAccountId): Result<TransactionOrigin> {
        return derivation.deriveTransactionSignerSource(accountId)
            .map(::SignedTransactionOrigin)
    }
}
