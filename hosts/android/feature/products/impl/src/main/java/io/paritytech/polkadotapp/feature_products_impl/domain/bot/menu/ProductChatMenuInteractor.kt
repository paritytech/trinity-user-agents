package io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import javax.inject.Inject

interface ProductChatMenuInteractor {
    suspend fun removeChat(productId: ProductId, chatId: ChatId): Result<Unit>
}

class RealProductChatMenuInteractor @Inject constructor(
    private val deleteRoomUseCase: DeleteRoomUseCase,
    private val productIntegrationRepository: ProductIntegrationRepository
) : ProductChatMenuInteractor {
    override suspend fun removeChat(productId: ProductId, chatId: ChatId): Result<Unit> {
        return runCatching {
            deleteRoomUseCase.invoke(chatId)
            productIntegrationRepository.uninstall(productId, IntegrationType.Chat)
        }
    }
}
