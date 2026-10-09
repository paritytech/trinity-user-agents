package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

/**
 * [ProductsBotApi] implementation for contexts without a chat channel (e.g. SPA browser).
 * All host-API operations are fully supported. [sendMessage] and [setWelcomeMessage] are no-ops.
 */
class ProductsBotApiImpl @AssistedInject constructor(
    hostApiInteractor: HostApiInteractor,
    @Assisted callingProductIdProvider: CallingProductIdProvider
) : BaseProductsBotApi(hostApiInteractor, callingProductIdProvider) {
    @AssistedFactory
    interface Factory {
        fun create(callingProductIdProvider: CallingProductIdProvider): ProductsBotApiImpl
    }

    override suspend fun createRoom(request: CreateProductRoomRequest): Result<CreateProductRoomResult> {
        return Result.failure(IllegalArgumentException("Creating a room is not supported in SPA"))
    }

    override suspend fun sendMessage(
        chatIdParameter: ProductChatIdParameter,
        message: ProductBotMessage
    ): Result<ChatMessageId> {
        return Result.failure(IllegalArgumentException("Sending a message is not supported in SPA"))
    }

    override fun subscribeChatRooms(): Flow<List<ProductChatRoom>> = emptyFlow()
}
