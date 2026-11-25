package io.paritytech.polkadotapp.feature_chats_impl.domain.models

sealed class ChatAvatar {
    /** [placeholder] is drawn until the image at [url] has loaded, and instead of it if it cannot. */
    class Url(val url: String, val placeholder: Account? = null) : ChatAvatar()

    class Account(val name: String, val themeSeed: ByteArray) : ChatAvatar()

    class PublicProfile(val username: String, val placeholder: Account) : ChatAvatar()

    companion object {
        /** The avatar [username]'s public profile names, over their letter. */
        fun publicProfile(username: String, themeSeed: ByteArray): ChatAvatar =
            PublicProfile(username, placeholder = Account(username, themeSeed))
    }
}
