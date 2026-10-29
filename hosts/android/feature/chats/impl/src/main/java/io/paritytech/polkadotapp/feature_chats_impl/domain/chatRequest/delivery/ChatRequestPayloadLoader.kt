package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestPayload
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenPlatform
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import javax.inject.Inject

/** Rebuilds what a recorded outgoing request carries from the contact, its welcome message and the current push token. */
class ChatRequestPayloadLoader @Inject constructor(
    private val chatEngine: ChatEngine,
    private val pushNotificationsHelper: PushNotificationsHelper,
) {
    suspend fun load(contact: Contact, request: ChatRequest): Result<OutgoingChatRequestPayload> = runCancellableCatching {
        OutgoingChatRequestPayload(
            contact = contact,
            pushToken = pushNotificationsHelper.getCurrentToken()?.toTokenContent(),
            welcomeMessage = welcomeMessageOf(contact, request),
        )
    }

    private suspend fun welcomeMessageOf(contact: Contact, request: ChatRequest): ChatMessage.Content.RichText? {
        val message = chatEngine.getMessageById(ChatId.fromContact(contact.accountId), request.id) ?: return null
        val content = message.content as? ChatMessage.Content.ChatRequest ?: return null

        return content.welcome
    }
}

fun String.toTokenContent(): TokenContent = TokenContent(
    token = toByteArray(Charsets.UTF_8),
    platform = TokenPlatform.ANDROID,
)
