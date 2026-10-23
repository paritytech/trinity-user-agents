package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update

/** The payment records in memory, with the transaction rule of the database: a throwing [insertSent] keeps nothing. */
class FakeProductChatPaymentStore : ProductChatPaymentStore {
    val records = MutableStateFlow<Map<String, ProductChatPayment>>(emptyMap())

    /** The statuses written for each message id, in order. */
    val statusWrites = mutableListOf<Pair<String, CoinagePayment.Status>>()

    fun put(payment: ProductChatPayment) = records.update { it + (payment.messageId to payment) }

    operator fun get(messageId: String): ProductChatPayment? = records.value[messageId]

    override suspend fun insertSent(payment: ProductChatPayment, onSaved: suspend () -> Unit) {
        check(payment.messageId !in records.value) { "the message id is taken" }
        onSaved()
        put(payment)
    }

    override suspend fun insertReceived(payment: ProductChatPayment) {
        if (payment.messageId !in records.value) put(payment)
    }

    override suspend fun adopt(payments: List<ProductChatPayment>): Int {
        val kept = payments.filter { it.messageId !in records.value }.distinctBy { it.messageId }
        records.update { all -> all + kept.associateBy { it.messageId } }
        return kept.size
    }

    override fun observeRoom(chatId: ChatId): Flow<List<ProductChatPayment>> = records.map { all -> all.values.filter { it.chatId == chatId } }

    override fun observeAll(): Flow<List<ProductChatPayment>> = records.map { it.values.toList() }

    override fun observeUnsettled(): Flow<List<ProductChatPayment>> = records.map { all -> all.values.filter { !it.settled } }

    override suspend fun sending(chatId: ChatId): List<ProductChatPayment> =
        records.value.values.filter { it.chatId == chatId && it.delivery == ProductChatPayment.Delivery.SENDING }.sortedBy { it.timestamp }

    override suspend fun roomsWithSending(): List<ChatId> =
        records.value.values.filter { it.delivery == ProductChatPayment.Delivery.SENDING }.map { it.chatId }.distinct()

    override suspend fun setDelivery(messageId: String, delivery: ProductChatPayment.Delivery) = change(messageId) { it.copy(delivery = delivery) }

    override suspend fun countRefusal(messageId: String): Int {
        change(messageId) { it.copy(refusals = it.refusals + 1) }
        return records.value[messageId]?.refusals ?: 0
    }

    override suspend fun setStatus(messageId: String, status: CoinagePayment.Status) {
        statusWrites += messageId to status
        change(messageId) { it.copy(status = status) }
    }

    override suspend fun settle(messageId: String) = change(messageId) { it.copy(settled = true) }

    private fun change(messageId: String, transform: (ProductChatPayment) -> ProductChatPayment) =
        records.update { all -> all[messageId]?.let { all + (messageId to transform(it)) } ?: all }
}
