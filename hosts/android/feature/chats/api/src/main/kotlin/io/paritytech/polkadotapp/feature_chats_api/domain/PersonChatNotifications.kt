package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAttachmentType

/**
 * Shows the pushes of a person chat of the chat product as the native chat shows its pushes: a message in the chat
 * channel, worded as the native chat words its kind, with the messages of the chat together and the chats grouped, and
 * nothing while the chat is on screen; an offer as the full-screen incoming call.
 */
interface PersonChatNotifications {
    suspend fun showMessage(chatId: ChatId, senderName: String, content: PersonChatPushContent)

    /** Rings the incoming call [callId] at once. A call rings once, also when its room sets it ringing later. */
    suspend fun ringCall(chatId: ChatId, callerName: String, callId: String, video: Boolean)
}

/** A pushed message of a kind that the native chat shows. */
sealed interface PersonChatPushContent {
    data class Text(val text: String) : PersonChatPushContent

    /** A message with a caption [text], and the kind of its first attachment when it has one. */
    data class RichText(val text: String?, val attachment: MessageAttachmentType?) : PersonChatPushContent

    data class Reaction(val emoji: String) : PersonChatPushContent

    data object ContactAdded : PersonChatPushContent

    data object RequestAccepted : PersonChatPushContent

    data class Payment(val totalValue: Balance) : PersonChatPushContent

    data object Unsupported : PersonChatPushContent
}
