package io.paritytech.polkadotapp.feature_chats_impl.domain.calls

import io.paritytech.polkadotapp.feature_calls_api.domain.IncomingCallGate
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageDirection
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.AlwaysFailCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import kotlinx.coroutines.flow.first
import javax.inject.Inject

class RealIncomingCallGate @Inject constructor(
    private val chatMessageRepository: ChatMessageRepository,
) : IncomingCallGate {
    override suspend fun shouldRing(chatId: ChatId, offerId: OfferId): Boolean {
        val hasCloseSignal = chatMessageRepository.subscribeMessages(
            chatId = chatId,
            direction = ChatMessageDirection.INCOMING,
            type = ChatMessage.Content.DataChannelClosed::class,
            status = ChatMessage.Status.NEW,
            customContentDecoder = AlwaysFailCustomContentDecoder()
        )
            .first()
            .any { (it.content as? ChatMessage.Content.DataChannelClosed)?.offerMessageId == offerId }

        return !hasCloseSignal
    }
}
