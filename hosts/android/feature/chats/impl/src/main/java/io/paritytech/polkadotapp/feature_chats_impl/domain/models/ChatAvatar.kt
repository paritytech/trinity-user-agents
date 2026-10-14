package io.paritytech.polkadotapp.feature_chats_impl.domain.models

sealed class ChatAvatar {
    class Url(val url: String) : ChatAvatar()

    class Account(val name: String, val themeSeed: ByteArray) : ChatAvatar()
}
