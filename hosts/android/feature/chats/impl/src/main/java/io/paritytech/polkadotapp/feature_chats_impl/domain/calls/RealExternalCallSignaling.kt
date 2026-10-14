package io.paritytech.polkadotapp.feature_chats_impl.domain.calls

import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.mapToUnit
import io.paritytech.polkadotapp.feature_calls_api.domain.EncodedIceCandidates
import io.paritytech.polkadotapp.feature_calls_api.domain.EncodedSpd
import io.paritytech.polkadotapp.feature_calls_api.domain.ExternalCallSignaling
import io.paritytech.polkadotapp.feature_calls_api.domain.IncomingOffer
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageDirection
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.subscribeMessages
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.asFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapConcat
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject

class RealExternalCallSignaling @Inject constructor(
    private val chatEngine: ChatEngine,
    private val chatMessageRepository: ChatMessageRepository
) : ExternalCallSignaling {
    override suspend fun sendOffer(offerId: OfferId, chatId: ChatId, sdp: EncodedSpd, withVideo: Boolean) {
        val purpose = if (withVideo) ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL else ChatMessage.Content.DataChannelOffer.Purpose.AUDIO_CALL
        chatEngine.sendUserMessage(
            messageId = offerId,
            chatId = chatId,
            content = ChatMessage.Content.DataChannelOffer(sdp, purpose)
        )
    }

    override suspend fun sendAnswer(
        offerId: OfferId,
        chatId: ChatId,
        sdp: EncodedSpd
    ) {
        chatEngine.sendUserMessage(
            chatId = chatId,
            content = ChatMessage.Content.DataChannelAnswer(offerId, sdp)
        )
    }

    override suspend fun sendIceCandidates(
        offerId: OfferId,
        chatId: ChatId,
        candidates: EncodedIceCandidates
    ) {
        chatEngine.sendUserMessage(
            chatId = chatId,
            content = ChatMessage.Content.DataChannelIceCandidate(offerId, candidates)
        )
    }

    override fun subscribeIncomingIceCandidates(
        chatId: ChatId,
        offerId: ChatMessageId
    ): Flow<EncodedIceCandidates> = chatEngine
        .subscribeMessages<ChatMessage.Content.DataChannelIceCandidate>(
            chatId = chatId,
            status = ChatMessage.Status.NEW,
            direction = ChatMessageDirection.INCOMING
        )
        .diffed()
        .flatMapConcat { it.added.asFlow() }
        .filter { (it.content as? ChatMessage.Content.DataChannelIceCandidate)?.offerMessageId == offerId }
        .onEach { message -> chatMessageRepository.markMessageAsRead(message.id) }
        .map { (it.content as ChatMessage.Content.DataChannelIceCandidate).sdp }

    override suspend fun awaitIncomingAnswer(
        offerId: OfferId,
        chatId: ChatId
    ): EncodedSpd = chatEngine
        .subscribeMessages<ChatMessage.Content.DataChannelAnswer>(
            chatId = chatId,
            status = ChatMessage.Status.NEW,
            direction = ChatMessageDirection.INCOMING
        ).mapNotNull { messages ->
            messages.firstOrNull {
                (it.content as? ChatMessage.Content.DataChannelAnswer)?.offerMessageId == offerId
            }
        }
        .onEach { message -> chatMessageRepository.markMessageAsRead(message.id) }
        .map { (it.content as ChatMessage.Content.DataChannelAnswer).sdp }
        .first()

    override suspend fun awaitIncomingOffer(offerId: OfferId): IncomingOffer {
        val message = chatEngine.awaitMessage(offerId)
        chatMessageRepository.markMessageAsRead(message.id)

        val offerContent = message.content as ChatMessage.Content.DataChannelOffer
        return IncomingOffer(
            sdp = offerContent.sdp,
            withVideo = offerContent.purpose == ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL
        )
    }

    override suspend fun sendCloseSignal(offerId: OfferId, chatId: ChatId) {
        chatEngine.sendUserMessage(
            chatId = chatId,
            content = ChatMessage.Content.DataChannelClosed(offerId)
        )
    }

    override fun subscribeIncomingCloseSignal(offerId: OfferId, chatId: ChatId) = chatEngine
        .subscribeMessages<ChatMessage.Content.DataChannelClosed>(
            chatId = chatId,
            status = ChatMessage.Status.NEW,
            direction = ChatMessageDirection.INCOMING
        )
        .mapNotNull { messages ->
            messages.firstOrNull {
                (it.content as? ChatMessage.Content.DataChannelClosed)?.offerMessageId == offerId
            }
        }
        .onEach { message -> chatMessageRepository.markMessageAsRead(message.id) }
        .mapToUnit()

    override fun observeOfferReadStatus(offerId: OfferId): Flow<Boolean> {
        return chatMessageRepository.subscribeMessageStatus(offerId)
            .map { it == ChatMessage.Status.IS_READ }
            .distinctUntilChanged()
    }
}
