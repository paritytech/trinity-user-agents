package io.paritytech.polkadotapp.feature_chats_api.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

interface ChatDeeplinkMapper {
    fun toDeeplink(chatId: ChatId): Uri

    fun fromDeeplink(data: Uri): Result<ChatDeeplinkPayload>
}

data class ChatDeeplinkPayload(
    val chatId: ChatId
)
