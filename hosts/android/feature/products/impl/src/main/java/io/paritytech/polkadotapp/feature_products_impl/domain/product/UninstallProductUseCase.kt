package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import javax.inject.Inject

class UninstallProductUseCase @Inject constructor(
    private val productRepository: ProductRepository,
    private val notificationScheduler: ProductNotificationScheduler,
    private val productGameReminder: ProductGameReminder,
) {
    suspend operator fun invoke(productId: ProductId): Result<Unit> {
        return notificationScheduler.cancelAllForProduct(productId)
            .map {
                productGameReminder.cancel(productId)
                productRepository.deleteProduct(productId)
            }
    }
}
