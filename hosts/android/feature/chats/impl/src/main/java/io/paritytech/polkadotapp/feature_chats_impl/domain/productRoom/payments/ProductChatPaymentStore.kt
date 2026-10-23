package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment
import kotlinx.coroutines.flow.Flow

/**
 * A payment in a room of the chat product, as the host keeps it: what a native payment message holds, under the same
 * message id. [delivery] is null for a payment that the contact sent.
 */
data class ProductChatPayment(
    val messageId: String,
    val chatId: ChatId,
    val incoming: Boolean,
    /** Milliseconds since the Unix epoch. */
    val timestamp: Long,
    val totalValue: Balance,
    val coinKeys: List<ByteArray>,
    val delivery: Delivery?,
    /** The times the product answered the payment with an error. Past a limit it does not go again. */
    val refusals: Int,
    val status: CoinagePayment.Status,
    /** Nothing about the payment can change any more: the coinage flow of its message is over. */
    val settled: Boolean,
) {
    enum class Delivery {
        /** Kept with its coins, and on its way to the product. */
        SENDING,
        DELIVERED,

        /** The product would not take the payment, so it does not go again. */
        REFUSED,
    }

    /** The payment waits to reach the product, or cannot reach it. */
    val isUndelivered: Boolean
        get() = delivery == Delivery.SENDING || delivery == Delivery.REFUSED
}

/** Keeps the payments of the rooms of the chat product, by message id. */
interface ProductChatPaymentStore {
    /** Keeps an outgoing payment. [onSaved] runs inside the transaction that keeps it; when it throws, nothing is kept. */
    suspend fun insertSent(payment: ProductChatPayment, onSaved: suspend () -> Unit)

    /** Keeps an incoming payment unless the store has its message id already. */
    suspend fun insertReceived(payment: ProductChatPayment)

    /** Keeps, in one transaction, each of [payments] whose message id the store does not have. Returns how many it kept. */
    suspend fun adopt(payments: List<ProductChatPayment>): Int

    fun observeRoom(chatId: ChatId): Flow<List<ProductChatPayment>>

    fun observeAll(): Flow<List<ProductChatPayment>>

    /** The payments whose coinage flow is not over, again after each change. */
    fun observeUnsettled(): Flow<List<ProductChatPayment>>

    /** The payments of the room that wait for the product, the oldest first. */
    suspend fun sending(chatId: ChatId): List<ProductChatPayment>

    suspend fun roomsWithSending(): List<ChatId>

    suspend fun setDelivery(messageId: String, delivery: ProductChatPayment.Delivery)

    /** Counts one error answer of the product, and gives the count. */
    suspend fun countRefusal(messageId: String): Int

    suspend fun setStatus(messageId: String, status: CoinagePayment.Status)

    suspend fun settle(messageId: String)
}
