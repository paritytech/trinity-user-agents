package io.paritytech.polkadotapp.feature_chats_impl.domain.mode

import android.content.SharedPreferences
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode

/** Debug builds run the chat product unless the user switched back; other builds run the native chat. */
class PrefsPersonChatMode(
    private val prefs: SharedPreferences,
    isDebugBuild: Boolean,
) : PersonChatMode {
    // Read once: the native services and the product decide at launch which one runs.
    private val productChat = prefs.getBoolean(KEY_PRODUCT_CHAT, isDebugBuild)

    override fun isProductChat(): Boolean = productChat

    override fun setProductChat(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_PRODUCT_CHAT, enabled).apply()
    }

    private companion object {
        const val KEY_PRODUCT_CHAT = "product_chat"
    }
}
