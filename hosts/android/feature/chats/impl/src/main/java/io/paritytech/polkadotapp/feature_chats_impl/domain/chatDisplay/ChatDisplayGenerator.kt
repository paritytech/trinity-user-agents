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
        val username = username ?: fallbackUsernameGenerator.generateFromAccountId(accountId)

        val avatar = if (avatarUrl != null) {
            ChatAvatar.Url(avatarUrl)
        } else {
            ChatAvatar.Account(username, themeSeed = accountId.value)
        }

        return ChatDisplay(username, avatar)
    }

    fun applyRoomMetadata(roomMetadata: RoomMetadata, defaultDisplay: ChatDisplay): ChatDisplay {
        val name = roomMetadata.name ?: defaultDisplay.name
        val avatar = roomMetadata.icon?.let(ChatAvatar::Url) ?: defaultDisplay.avatar

        return ChatDisplay(name, avatar)
    }
}
