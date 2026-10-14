package io.paritytech.polkadotapp.feature_chats_impl.deeplink

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.deeplink.ChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_api.deeplink.ChatDeeplinkPayload
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import javax.inject.Inject

internal class RealChatDeeplinkMapper @Inject constructor() : ChatDeeplinkMapper {
    override fun toDeeplink(chatId: ChatId): Uri {
        return Uri.Builder()
            .scheme(DeepLinkHandler.APP_SCHEME)
            .authority(CHAT_HOST)
            .appendQueryParameter(CHAT_PARAM_ID, chatId.value.value.toHexString(withPrefix = false))
            .build()
    }

    override fun fromDeeplink(data: Uri): Result<ChatDeeplinkPayload> {
        val chatIdHex = data.getQueryParameter(CHAT_PARAM_ID)
            ?: return Result.failure(IllegalArgumentException("Missing '$CHAT_PARAM_ID' in chat deeplink"))

        return runCancellableCatching {
            val chatId = ChatId.fromRawValue(chatIdHex.fromHex())

            ChatDeeplinkPayload(chatId = chatId)
        }
    }
}
