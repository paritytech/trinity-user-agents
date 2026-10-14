package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toUi
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.toAvatar
import javax.inject.Inject

class ChatListRoomPolicyMapper @Inject constructor(
    private val resourceResolver: ChatResourceResolver,
) {
    /** The preview text that the product set, or null when the row shows the last message. */
    fun productPreview(chat: Chat): LastMessageUiModel.Text? {
        val preview = chat.roomPolicy?.listPreview as? ChatRoomPolicy.ListPreview.Text ?: return null

        return LastMessageUiModel.Text(
            timestamp = chat.roomPolicy.listOrder.timestamp ?: chat.timestamp,
            isIncoming = true,
            message = preview.text,
        )
    }

    fun isOngoing(chat: Chat): Boolean {
        val preview = chat.roomPolicy?.listPreview as? ChatRoomPolicy.ListPreview.Text ?: return false

        return preview.ongoing
    }

    suspend fun badge(badge: ChatSummaryBadge): ChatListUiState.Badge {
        return when (badge) {
            is ChatSummaryBadge.Notification,
            is ChatSummaryBadge.None -> ChatListUiState.Badge.None

            is ChatSummaryBadge.Unread -> ChatListUiState.Badge.Unread(badge.count)

            is ChatSummaryBadge.Image -> resourceResolver.resolve(badge.resource)
                .logFailure("Failed to resolve the badge image of a product room")
                .fold(onSuccess = { ChatListUiState.Badge.Image(it) }, onFailure = { ChatListUiState.Badge.None })
        }
    }

    /** The avatar that the product set has priority over the icon of the room. */
    suspend fun avatar(chat: Chat): AvatarUiModel {
        val hostAvatar = hostAvatar(chat.display.avatar)
        val resource = chat.roomPolicy?.avatar ?: return hostAvatar

        return resourceResolver.resolve(resource)
            .logFailure("Failed to resolve the avatar of a product room")
            .fold(onSuccess = { it.toAvatar() }, onFailure = { hostAvatar })
    }

    /** The image loader cannot read a `data:` icon of the first version, so the resolver decodes it. */
    private fun hostAvatar(avatar: ChatAvatar): AvatarUiModel {
        if (avatar !is ChatAvatar.Url) return avatar.toUi()

        return resourceResolver.resolveIcon(avatar.url)
            .logFailure("Failed to resolve the icon of a room")
            .fold(onSuccess = { it.toAvatar() }, onFailure = { avatar.toUi() })
    }
}
