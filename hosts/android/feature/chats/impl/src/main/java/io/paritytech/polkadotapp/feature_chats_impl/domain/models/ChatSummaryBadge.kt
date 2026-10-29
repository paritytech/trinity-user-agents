package io.paritytech.polkadotapp.feature_chats_impl.domain.models

sealed interface ChatSummaryBadge {
    data class Unread(val count: Int) : ChatSummaryBadge

    object Notification : ChatSummaryBadge

    object None : ChatSummaryBadge

    /** An image that a product shows in place of the unread count. */
    data class Image(val icon: String) : ChatSummaryBadge
}
