package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.roomAvatar
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

    suspend fun avatar(chat: Chat): AvatarUiModel = resourceResolver.roomAvatar(chat.display.avatar, chat.roomPolicy?.avatar)
}
