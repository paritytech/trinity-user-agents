package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order

private val recencyOrder: Comparator<ChatSummary> = compareByDescending<ChatSummary> { it.lastMessageSortOrder }
    .thenByDescending { it.timestamp }

/**
 * The order of the chat list: the pinned rooms first, then the others, each part by recency.
 *
 * The last messages have a local sort order, and a product sets a timestamp in milliseconds for a room whose messages
 * it keeps. One comparator cannot sort the two together, so this is a merge of two sorted lists (02-DESIGN §19 item 2).
 * The chats that sort by their last message keep [recencyOrder]. The merge puts a room with a product timestamp before
 * the first of those chats whose last message is older than the timestamp.
 */
internal fun chatListOrder(summaries: List<ChatSummary>, productTimestamps: Map<ChatId, Long>): List<ChatSummary> {
    val (pinned, others) = summaries.partition { it.order == Order.PinToTop }

    return byRecency(pinned, productTimestamps) + byRecency(others, productTimestamps)
}

private fun byRecency(summaries: List<ChatSummary>, productTimestamps: Map<ChatId, Long>): List<ChatSummary> {
    val byMessage = summaries.filter { it.chatId !in productTimestamps }.sortedWith(recencyOrder)
    val byProduct = summaries
        .mapNotNull { summary -> productTimestamps[summary.chatId]?.let { summary to it } }
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
