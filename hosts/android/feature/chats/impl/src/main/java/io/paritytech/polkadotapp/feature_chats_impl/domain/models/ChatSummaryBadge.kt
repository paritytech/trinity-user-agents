package io.paritytech.polkadotapp.feature_chats_impl.domain.models

sealed interface ChatSummaryBadge {
    data class Unread(val count: Int) : ChatSummaryBadge

    object Notification : ChatSummaryBadge

    object None : ChatSummaryBadge
}
