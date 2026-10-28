package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.EncryptedAuthTokenStore
import javax.inject.Inject

class UninstallProductUseCase @Inject constructor(
    private val productRepository: ProductRepository,
    private val notificationScheduler: ProductNotificationScheduler,
    private val productGameReminder: ProductGameReminder,
    private val authTokens: EncryptedAuthTokenStore,
) {
    /** The product's auth tokens go with it, so a reinstall signs in again. */
    suspend operator fun invoke(productId: ProductId): Result<Unit> {
        return notificationScheduler.cancelAllForProduct(productId)
            .mapCatching { authTokens.clearAuthTokens(productId.value) }
            .map {
                productGameReminder.cancel(productId)
                productRepository.deleteProduct(productId)
            }
    }
}
