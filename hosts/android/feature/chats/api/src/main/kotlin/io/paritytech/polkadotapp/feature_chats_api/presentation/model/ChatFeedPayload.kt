package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import android.os.Parcelable
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import kotlinx.parcelize.Parcelize

@Parcelize
class ChatFeedPayload private constructor(
    val openChatRequest: OpenChatRequestParcelable,
    val targetMessageId: ChatMessageId?,
) : Parcelable {
    companion object {
        fun existingChat(
            chatId: ChatId,
            targetMessageId: ChatMessageId? = null
        ): ChatFeedPayload {
            val request = OpenChatRequestParcelable.ExistingChat(chatId.toParcelable())
            return ChatFeedPayload(request, targetMessageId)
        }

        fun existingContactChat(contactAccountId: AccountId): ChatFeedPayload {
            return existingChat(ChatId.fromContact(contactAccountId))
        }

        fun botChat(botId: ChatExtensionId): ChatFeedPayload {
            return existingChat(ChatId.fromChatBotId(botId))
        }

        fun startChatWithContact(
            contactAccountId: AccountId,
            username: Username?,
            avatar: String?,
            chatKey: X25519PublicKey,
            sharedSecretDerivationDomain: SharedSecretDerivationDomain,
            ourMetaAccountId: Long,
            origin: ContactOrigin,
        ): ChatFeedPayload {
            val request = OpenChatRequestParcelable.StartChatWithContact(
                contactAccountId = contactAccountId.value,
                username = username?.getDisplayUsername(),
                avatar = avatar,
                sharedSecretDerivationDomain = sharedSecretDerivationDomain.domain,
                chatKey = chatKey.bytes.value,
                ourMetaAccountId = ourMetaAccountId,
                origin = origin
            )
            return ChatFeedPayload(request, targetMessageId = null)
        }
    }
}

@Parcelize
sealed class OpenChatRequestParcelable : Parcelable {
    @Parcelize
    class ExistingChat(val chatIdParcel: ChatIdParcel) : OpenChatRequestParcelable()

    @Parcelize
    class StartChatWithContact(
        val contactAccountId: ByteArray,
        val username: String?,
        val avatar: String?,
        val chatKey: ByteArray,
        val sharedSecretDerivationDomain: String,
        val ourMetaAccountId: Long,
        val origin: String,
    ) : OpenChatRequestParcelable()
}

@Parcelize
class ChatIdParcel(val value: ByteArray) : Parcelable

fun ChatFeedPayload.toOpenChatRequest(): OpenChatRequest {
    return when (val request = openChatRequest) {
        is OpenChatRequestParcelable.ExistingChat -> {
            OpenChatRequest.ExistingChat(request.chatIdParcel.toChatId())
        }
        is OpenChatRequestParcelable.StartChatWithContact -> {
            OpenChatRequest.StartChatWithContact(
                contactAccountId = AccountId(request.contactAccountId),
                username = request.username?.let(Username::fromFullValue),
                avatar = request.avatar,
                chatKey = request.chatKey,
                sharedSecretDerivationDomain = SharedSecretDerivationDomain(request.sharedSecretDerivationDomain),
                ourMetaAccountId = request.ourMetaAccountId,
                origin = request.origin
            )
        }
    }
}

private fun ChatIdParcel.toChatId(): ChatId {
    return ChatId.fromRawValue(value)
}

private fun ChatId.toParcelable(): ChatIdParcel {
    return ChatIdParcel(value.value)
}
