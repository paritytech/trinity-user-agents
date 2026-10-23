package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.payments

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.database.dao.ProductChatPaymentDao
import io.paritytech.polkadotapp.database.model.ProductChatPaymentLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment.Status
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPayment
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPaymentStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RoomProductChatPaymentStore @Inject constructor(
    private val dao: ProductChatPaymentDao,
) : ProductChatPaymentStore {
    override suspend fun insertSent(payment: ProductChatPayment, onSaved: suspend () -> Unit) {
        dao.insertSent(payment.toLocal(), onSaved)
    }

    override suspend fun insertReceived(payment: ProductChatPayment) {
        dao.insertIfAbsent(payment.toLocal())
    }

    override fun observeRoom(chatId: ChatId): Flow<List<ProductChatPayment>> =
        dao.observeRoom(chatId.value.value).map { list -> list.map { it.toDomain() } }

    override fun observeAll(): Flow<List<ProductChatPayment>> = dao.observeAll().map { list -> list.map { it.toDomain() } }

    override fun observeUnsettled(): Flow<List<ProductChatPayment>> = dao.observeUnsettled().map { list -> list.map { it.toDomain() } }

    override suspend fun sending(chatId: ChatId): List<ProductChatPayment> = dao.getSending(chatId.value.value).map { it.toDomain() }

    override suspend fun roomsWithSending(): List<ChatId> = dao.getRoomsWithSending().map(ChatId::fromRawValue)

    override suspend fun setDelivery(messageId: String, delivery: ProductChatPayment.Delivery) {
        dao.setDelivery(messageId, delivery.toLocal())
    }

    override suspend fun countRefusal(messageId: String): Int {
        dao.countRefusal(messageId)
        return dao.get(messageId)?.refusals ?: 0
    }

    override suspend fun setStatus(messageId: String, status: Status) {
        val (local, amount) = status.toLocal()
        dao.setStatus(messageId, local, amount?.value)
    }

    override suspend fun settle(messageId: String) {
        dao.settle(messageId)
    }
}

private fun ProductChatPayment.toLocal(): ProductChatPaymentLocal {
    val (status, amount) = status.toLocal()
    return ProductChatPaymentLocal(
        messageId = messageId,
        chatId = chatId.value.value,
        incoming = incoming,
        timestamp = timestamp,
        totalValue = totalValue.value,
        coinKeys = coinKeys.joinToString(separator = KEY_SEPARATOR) { it.toHexString() },
        delivery = delivery?.toLocal(),
        refusals = refusals,
        status = status,
        statusAmount = amount?.value,
        settled = settled,
    )
}

private fun ProductChatPaymentLocal.toDomain() = ProductChatPayment(
    messageId = messageId,
    chatId = ChatId.fromRawValue(chatId),
    incoming = incoming,
    timestamp = timestamp,
    totalValue = Balance(totalValue),
    coinKeys = coinKeys.split(KEY_SEPARATOR).filter { it.isNotEmpty() }.map { it.fromHex() },
    delivery = delivery?.toDomain(),
    refusals = refusals,
    status = toStatus(),
    settled = settled,
)

private fun Status.toLocal(): Pair<ProductChatPaymentLocal.Status, Balance?> = when (this) {
    Status.Detecting -> ProductChatPaymentLocal.Status.DETECTING to null
    is Status.Detected -> ProductChatPaymentLocal.Status.DETECTED to amount
    is Status.PartiallyClaimed -> ProductChatPaymentLocal.Status.PARTIALLY_CLAIMED to claimed
    is Status.Transferred -> ProductChatPaymentLocal.Status.TRANSFERRED to amount
    Status.FailedDetection -> ProductChatPaymentLocal.Status.FAILED_DETECTION to null
    Status.FailedTransfer -> ProductChatPaymentLocal.Status.FAILED_TRANSFER to null
}

private fun ProductChatPaymentLocal.toStatus(): Status {
    val amount = Balance(statusAmount ?: totalValue)
    return when (status) {
        ProductChatPaymentLocal.Status.DETECTING -> Status.Detecting
        ProductChatPaymentLocal.Status.DETECTED -> Status.Detected(amount)
        ProductChatPaymentLocal.Status.PARTIALLY_CLAIMED -> Status.PartiallyClaimed(amount)
        ProductChatPaymentLocal.Status.TRANSFERRED -> Status.Transferred(amount)
        ProductChatPaymentLocal.Status.FAILED_DETECTION -> Status.FailedDetection
        ProductChatPaymentLocal.Status.FAILED_TRANSFER -> Status.FailedTransfer
    }
}

private fun ProductChatPayment.Delivery.toLocal() = when (this) {
    ProductChatPayment.Delivery.SENDING -> ProductChatPaymentLocal.Delivery.SENDING
    ProductChatPayment.Delivery.DELIVERED -> ProductChatPaymentLocal.Delivery.DELIVERED
    ProductChatPayment.Delivery.REFUSED -> ProductChatPaymentLocal.Delivery.REFUSED
}

private fun ProductChatPaymentLocal.Delivery.toDomain() = when (this) {
    ProductChatPaymentLocal.Delivery.SENDING -> ProductChatPayment.Delivery.SENDING
    ProductChatPaymentLocal.Delivery.DELIVERED -> ProductChatPayment.Delivery.DELIVERED
    ProductChatPaymentLocal.Delivery.REFUSED -> ProductChatPayment.Delivery.REFUSED
}

private const val KEY_SEPARATOR = ","
