package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.ProductAccountDerivationUseCase
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import javax.inject.Inject

/**
 * Turns the source a product named into something that can actually sign or be claimed.
 *
 * Its own class because a top-up outlives the call that asked for it: the same resolution has to happen
 * again, from the recorded bytes, whenever a top-up is picked back up in a later process.
 */
class TopUpSourceResolver @Inject constructor(
    private val productAccountDerivationUseCase: ProductAccountDerivationUseCase,
    private val signedOrigins: SignedOrigins,
) {
    suspend fun resolve(productId: ProductId, source: PaymentTopUpSource): Result<TopUpSource> = when (source) {
        is PaymentTopUpSource.ProductAccount -> {
            val productAccountId = ProductAccountId(productId = productId.value, index = source.index)

            productAccountDerivationUseCase.deriveTransactionSignerSource(productAccountId)
                .map { TopUpSource.Onboard(it) }
        }

        is PaymentTopUpSource.PrivateKey -> signedOrigins.signedTransactionSourceSr25519PrivateKey(source.key)
            .map { TopUpSource.Onboard(it) }

        is PaymentTopUpSource.Coins -> Result.success(TopUpSource.Coins(source.secretKeys))
    }
}
