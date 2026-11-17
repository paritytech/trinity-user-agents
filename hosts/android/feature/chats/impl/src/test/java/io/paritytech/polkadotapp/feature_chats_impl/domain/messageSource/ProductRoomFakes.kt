package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger

internal val productRoom = ChatId.forExtensionRoom("ProductBot_weather", "lobby")

internal fun productMessage(
    index: Int,
    id: String = "m$index",
    participant: String = "product",
    timestamp: Long = index * 1_000L,
    status: ProductMessageStatusModel = ProductMessageStatusModel.SENT,
) = ProductRoomMessage(
    messageId = id,
    participantId = participant,
    timestamp = timestamp,
    content = ChatMessage.Content.Text("message $index"),
    status = status,
    replyTo = null,
    editedAt = null,
    reactions = emptyList(),
    request = false,
)

/** The pages of a product, as a test writes them. It keeps every request. */
internal class ScriptedProductChatQuerying(
    private val answerPage: suspend (ChatPageAnchorModel, Int) -> Result<ProductMessagesPage> = { _, _ ->
        Result.success(ProductMessagesPage(emptyList(), next = null))
    },
) : ProductChatQuerying {
    val pageRequests = CopyOnWriteArrayList<ChatPageAnchorModel>()

    override suspend fun messagesPage(chatId: ChatId, anchor: ChatPageAnchorModel, pageSize: Int): Result<ProductMessagesPage> {
        pageRequests += anchor
        return answerPage(anchor, pageSize)
    }
}

/** A room of the product with slot cursors: a cursor is the slot where the page ends. */
internal class ProductRoomHistory(count: Int, private val participant: String = "product") {
    private val count = AtomicInteger(count)

    fun append(added: Int) {
        count.addAndGet(added)
    }

    fun ids(range: IntRange): List<String> = range.map { "m$it" }

    val answer: suspend (ChatPageAnchorModel, Int) -> Result<ProductMessagesPage> = { anchor, size -> Result.success(page(anchor, size)) }

    private fun page(anchor: ChatPageAnchorModel, size: Int): ProductMessagesPage {
        val total = count.get()

        return when (anchor) {
            is ChatPageAnchorModel.Before -> {
                val end = anchor.cursor?.toInt() ?: total
                val start = (end - size).coerceAtLeast(0)
                ProductMessagesPage((start until end).map { productMessage(it, participant = participant) }, next = start.takeIf { it > 0 }?.toString())
            }

            is ChatPageAnchorModel.After -> {
                val start = anchor.cursor?.toInt() ?: 0
                val end = (start + size).coerceAtMost(total)
                ProductMessagesPage((start until end).map { productMessage(it, participant = participant) }, next = end.takeIf { it < total }?.toString())
            }
        }
    }
}
