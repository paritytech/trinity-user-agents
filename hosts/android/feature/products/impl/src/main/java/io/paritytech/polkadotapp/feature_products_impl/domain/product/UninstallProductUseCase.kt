package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import javax.inject.Inject

class UninstallProductUseCase @Inject constructor(
    private val productRepository: ProductRepository,
    private val notificationScheduler: ProductNotificationScheduler,
    private val productGameReminder: ProductGameReminder,
    private val deleteRoomUseCase: DeleteRoomUseCase,
) {
    // Uninstalling a product removes all its chat rooms (02-DESIGN §5.5). Its chat integration goes with the product row.
    suspend operator fun invoke(productId: ProductId): Result<Unit> {
        return notificationScheduler.cancelAllForProduct(productId)
            .mapCatching {
                productGameReminder.cancel(productId)
                deleteRoomUseCase.deleteAllRoomsOf(productId.toChatExtensionId())
                productRepository.deleteProduct(productId)
            }
    }
}
