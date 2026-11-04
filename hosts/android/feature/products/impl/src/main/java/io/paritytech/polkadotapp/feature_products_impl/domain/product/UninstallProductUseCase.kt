package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import uniffi.truapi.ProductId
import javax.inject.Inject

class UninstallProductUseCase @Inject constructor(
    private val productRepository: ProductRepository,
    private val notificationScheduler: ProductNotificationScheduler,
    private val productGameReminder: ProductGameReminder,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) {
    /** The product stays installed when its state cannot be cleared, so removing it again retries. */
    suspend operator fun invoke(productId: ProductId): Result<Unit> {
        return notificationScheduler.cancelAllForProduct(productId)
            .mapCatching { runtimeProvider.runtime().getOrThrow().clearProductState(productId) }
            .map {
                productGameReminder.cancel(productId)
                productRepository.deleteProduct(productId)
            }
    }
}
