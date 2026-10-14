package io.paritytech.polkadotapp.feature_chats_api.domain.model

sealed class ContactAvatar {
    class Url(val url: String) : ContactAvatar()

    class Account(val name: String, val themeSeed: ByteArray) : ContactAvatar()
}
