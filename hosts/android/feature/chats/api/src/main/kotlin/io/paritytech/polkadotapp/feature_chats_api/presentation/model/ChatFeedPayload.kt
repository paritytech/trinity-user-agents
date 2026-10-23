package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import android.os.Parcelable
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
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
    }
}

@Parcelize
sealed class OpenChatRequestParcelable : Parcelable {
    @Parcelize
    class ExistingChat(val chatIdParcel: ChatIdParcel) : OpenChatRequestParcelable()
}

@Parcelize
class ChatIdParcel(val value: ByteArray) : Parcelable

fun ChatFeedPayload.toOpenChatRequest(): OpenChatRequest {
    return when (val request = openChatRequest) {
        is OpenChatRequestParcelable.ExistingChat -> {
            OpenChatRequest.ExistingChat(request.chatIdParcel.toChatId())
        }
    }
}

private fun ChatIdParcel.toChatId(): ChatId {
    return ChatId.fromRawValue(value)
}

private fun ChatId.toParcelable(): ChatIdParcel {
    return ChatIdParcel(value.value)
}
