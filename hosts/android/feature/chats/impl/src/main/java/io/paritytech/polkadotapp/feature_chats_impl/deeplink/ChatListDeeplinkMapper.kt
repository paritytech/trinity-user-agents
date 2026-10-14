package io.paritytech.polkadotapp.feature_chats_impl.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import javax.inject.Inject

internal class ChatListDeeplinkMapper @Inject constructor() {
    fun toDeeplink(): Uri {
        return Uri.Builder()
            .scheme(DeepLinkHandler.APP_SCHEME)
            .authority(CHAT_LIST_HOST)
            .build()
    }
}
