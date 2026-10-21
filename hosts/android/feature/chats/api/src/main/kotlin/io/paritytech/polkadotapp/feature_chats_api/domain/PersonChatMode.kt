package io.paritytech.polkadotapp.feature_chats_api.domain

/**
 * Which chat runs the user's person-to-person chats on this install (HX2): the native chat or the chat product, never
 * both, since both would answer the same statements and sessions. A change takes effect at the next launch.
 */
interface PersonChatMode {
    /** The mode this launch runs in. */
    fun isProductChat(): Boolean

    fun setProductChat(enabled: Boolean)
}
