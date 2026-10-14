package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy

private val recencyOrder: Comparator<ChatSummary> = compareByDescending<ChatSummary> { it.lastMessageSortOrder }
    .thenByDescending { it.timestamp }

/**
 * The order of the chat list: the pinned rooms first, then the others, each part by recency.
 *
 * The last messages have a local sort order, and a product sets a timestamp in milliseconds. One comparator
 * cannot sort the two together, so this is a merge of two sorted lists (02-DESIGN §19 item 2). The chats that
 * sort by their last message keep [recencyOrder]. The merge puts a room with a product timestamp before the
 * first of those chats whose last message is older than the timestamp.
 */
internal fun chatListOrder(
    summaries: List<ChatSummary>,
    listOrders: Map<ChatId, ChatRoomPolicy.ListOrder>,
): List<ChatSummary> {
    val (pinned, others) = summaries.partition { it.order == Order.PinToTop || listOrders[it.chatId]?.isPinned == true }

    return byRecency(pinned, listOrders) + byRecency(others, listOrders)
}

private fun byRecency(
    summaries: List<ChatSummary>,
    listOrders: Map<ChatId, ChatRoomPolicy.ListOrder>,
): List<ChatSummary> {
    val byMessage = summaries.filter { listOrders[it.chatId]?.timestamp == null }.sortedWith(recencyOrder)
    val byProduct = summaries
        .mapNotNull { summary -> listOrders[summary.chatId]?.timestamp?.let { summary to it } }
        .sortedByDescending { (_, timestamp) -> timestamp }

    val result = ArrayList<ChatSummary>(summaries.size)
    var messageIndex = 0

    for ((summary, timestamp) in byProduct) {
        while (messageIndex < byMessage.size && byMessage[messageIndex].hasMessageNewerThan(timestamp)) {
            result += byMessage[messageIndex]
            messageIndex++
        }
        result += summary
    }

    result += byMessage.subList(messageIndex, byMessage.size)
    return result
}

private fun ChatSummary.hasMessageNewerThan(timestamp: Long): Boolean {
    return lastMessageSortOrder != null && this.timestamp > timestamp
}

/** A product badge replaces the host count and the reaction mark (02-DESIGN §19 item 3). */
internal fun ChatSummary.withRoomPolicy(policy: ChatRoomPolicy?): ChatSummary {
    if (policy == null || !policy.replacesHostBadge) return this

    val productBadge = when (val badge = policy.listBadge) {
        ChatRoomPolicy.ListBadge.HostUnreadCount -> this.badge
        ChatRoomPolicy.ListBadge.Hidden -> ChatSummaryBadge.None
        is ChatRoomPolicy.ListBadge.Count -> if (badge.count > 0) ChatSummaryBadge.Unread(badge.count) else ChatSummaryBadge.None
        is ChatRoomPolicy.ListBadge.Image -> ChatSummaryBadge.Image(badge.resource)
    }

    return copy(badge = productBadge, hasUnseenReaction = false)
}
