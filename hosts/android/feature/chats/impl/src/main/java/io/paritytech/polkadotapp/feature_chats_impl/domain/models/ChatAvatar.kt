package io.paritytech.polkadotapp.feature_chats_impl.domain.models

sealed class ChatAvatar {
    /** [placeholder] is drawn until the image at [url] has loaded, and instead of it if it cannot. */
    class Url(val url: String, val placeholder: Account?) : ChatAvatar()

    class Account(val name: String, val themeSeed: ByteArray) : ChatAvatar()
}
