package io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ChatDisplayGenerator @Inject constructor(
    private val fallbackUsernameGenerator: FallbackUsernameGenerator,
) {
    fun generateAccountChatDisplay(
        accountId: AccountId,
        username: String?,
        avatarUrl: String?,
    ): ChatDisplay {
        val displayName = username ?: fallbackUsernameGenerator.generateFromAccountId(accountId)

        val avatar = when {
            avatarUrl != null -> ChatAvatar.Url(avatarUrl)
            username != null -> ChatAvatar.publicProfile(username, themeSeed = accountId.value)
            else -> ChatAvatar.Account(displayName, themeSeed = accountId.value)
        }

        return ChatDisplay(displayName, avatar)
    }

    /** A room icon is drawn over the default letter, which stays if the icon cannot be loaded. */
    fun applyRoomMetadata(roomMetadata: RoomMetadata, defaultDisplay: ChatDisplay): ChatDisplay {
        val name = roomMetadata.name ?: defaultDisplay.name
        val avatar = roomMetadata.icon
            ?.let { ChatAvatar.Url(it, placeholder = defaultDisplay.avatar.letter()) }
            ?: defaultDisplay.avatar

        return ChatDisplay(name, avatar)
    }

    private fun ChatAvatar.letter(): ChatAvatar.Account? = when (this) {
        is ChatAvatar.Account -> this
        is ChatAvatar.Url -> placeholder
        is ChatAvatar.PublicProfile -> placeholder
    }
}
