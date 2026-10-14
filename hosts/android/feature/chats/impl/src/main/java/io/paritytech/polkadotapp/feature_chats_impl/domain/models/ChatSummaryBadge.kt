package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel

sealed interface ChatSummaryBadge {
    data class Unread(val count: Int) : ChatSummaryBadge

    data class Image(val resource: ChatResourceModel) : ChatSummaryBadge

    object Notification : ChatSummaryBadge

    object None : ChatSummaryBadge
}
