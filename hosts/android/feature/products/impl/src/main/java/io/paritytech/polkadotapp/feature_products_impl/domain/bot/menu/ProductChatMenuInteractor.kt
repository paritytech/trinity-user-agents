package io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu

import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import kotlinx.coroutines.flow.first
import javax.inject.Inject

interface ProductChatMenuInteractor {
    suspend fun removeChat(productId: ProductId, chatId: ChatId): Result<Unit>
}

class RealProductChatMenuInteractor @Inject constructor(
    private val deleteRoomUseCase: DeleteRoomUseCase,
    private val chatRoomRepository: ReadOnlyChatRoomRepository,
    private val productIntegrationRepository: ProductIntegrationRepository
) : ProductChatMenuInteractor {
    override suspend fun removeChat(productId: ProductId, chatId: ChatId): Result<Unit> {
        return runCatching {
            deleteRoomUseCase.invoke(chatId)

            // The integration keeps the product's worker driving its other rooms. With none
            // left it would only boot to recreate the room the user just removed.
            val remainingRooms = chatRoomRepository.subscribeRoomsByExtension(productId.toChatExtensionId()).first()
            if (remainingRooms.isEmpty()) {
                productIntegrationRepository.uninstall(productId, IntegrationType.Chat)
            }
        }
    }
}
