package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRegistration
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import javax.inject.Inject

/** Makes a [ProductMessageSource] that takes the live feed updates of its room until it closes. */
class RealProductMessageSourceFactory @Inject constructor(
    private val feedListeners: ProductFeedListeners,
    private val cache: ProductMessageCaching,
    private val dispatchers: CoroutineDispatchers,
) : ProductMessageSourceFactory {
    override fun create(chatId: ChatId, policy: ChatRoomPolicy, querying: ProductChatQuerying?): ChatMessageSource {
        val source = ProductMessageSource(
            chatId = chatId,
            querying = querying ?: MissingProductChatQuerying,
            cache = cache,
            settings = ProductMessageSource.Settings.Default,
            dispatcher = dispatchers.computation,
        )

        return FeedRegisteredSource(source, feedListeners.register(chatId, source))
    }
}

private class FeedRegisteredSource(
    private val source: ProductMessageSource,
    private val registration: ProductFeedRegistration,
) : ChatMessageSource by source {
    override fun close() {
        registration.cancel()
        source.close()
    }
}

/** The queries of a product that does not serve product-source rooms. Each query fails as unavailable. */
internal object MissingProductChatQuerying : ProductChatQuerying {
    override suspend fun messagesPage(chatId: ChatId, anchor: ChatPageAnchorModel, pageSize: Int): Result<ProductMessagesPage> =
        Result.failure(ProductChatQueryException.Unavailable(null))
}
