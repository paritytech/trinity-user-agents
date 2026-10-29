package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.database.dao.ChatAcceptanceDao
import io.paritytech.polkadotapp.database.model.ChatAcceptanceLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.acceptance.ChatAcceptance
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject
import kotlin.time.Instant

interface ChatAcceptanceRepository {
    suspend fun save(acceptance: ChatAcceptance)

    suspend fun get(contactAccountId: AccountId): ChatAcceptance?

    /** Acceptances not yet published, of contacts that are not blocked. */
    fun subscribeUndelivered(): Flow<List<ChatAcceptance>>

    /** Published acceptances still awaiting the contact, of contacts that are not blocked. */
    suspend fun getDelivered(): List<ChatAcceptance>

    /** No-op when the stored acceptance of the contact is no longer [acceptance]'s message. */
    suspend fun updateDelivery(acceptance: ChatAcceptance, delivery: ChatRequest.Delivery)

    suspend fun remove(contactAccountId: AccountId)
}

class RealChatAcceptanceRepository @Inject constructor(
    private val dao: ChatAcceptanceDao,
) : ChatAcceptanceRepository {
    override suspend fun save(acceptance: ChatAcceptance) {
        dao.upsert(acceptance.toLocal())
    }

    override suspend fun get(contactAccountId: AccountId): ChatAcceptance? {
        return dao.get(contactAccountId.value)?.toDomain()
    }

    override fun subscribeUndelivered(): Flow<List<ChatAcceptance>> {
        return dao.subscribeUndelivered().mapList { it.toDomain() }
    }

    override suspend fun getDelivered(): List<ChatAcceptance> {
        return dao.getDelivered().map { it.toDomain() }
    }

    override suspend fun updateDelivery(acceptance: ChatAcceptance, delivery: ChatRequest.Delivery) {
        val local = delivery.toLocal()
        dao.updateDelivery(
            contactAccountId = acceptance.contactAccountId.value,
            messageId = acceptance.messageId,
            deliveryStatus = local.status,
            deliveredVia = local.via,
            lastDeliveredPeriod = local.period,
        )
    }

    override suspend fun remove(contactAccountId: AccountId) {
        dao.delete(contactAccountId.value)
    }

    private class LocalDelivery(
        val status: ChatAcceptanceLocal.DeliveryStatus,
        val via: ChatAcceptanceLocal.DeliveredVia?,
        val period: Long?,
    )

    private fun ChatAcceptance.toLocal(): ChatAcceptanceLocal {
        val local = delivery.toLocal()
        return ChatAcceptanceLocal(
            contactAccountId = contactAccountId.value,
            requestId = requestId,
            messageId = messageId,
            acceptedAt = acceptedAt.toEpochMilliseconds(),
            deliveryStatus = local.status,
            deliveredVia = local.via,
            lastDeliveredPeriod = local.period,
        )
    }

    private fun ChatRequest.Delivery.toLocal(): LocalDelivery = when (this) {
        ChatRequest.Delivery.Undelivered -> LocalDelivery(ChatAcceptanceLocal.DeliveryStatus.UNDELIVERED, via = null, period = null)

        ChatRequest.Delivery.Failed -> LocalDelivery(ChatAcceptanceLocal.DeliveryStatus.FAILED, via = null, period = null)

        ChatRequest.Delivery.DeliveredUntracked ->
            LocalDelivery(ChatAcceptanceLocal.DeliveryStatus.DELIVERED, ChatAcceptanceLocal.DeliveredVia.USERNAME, period = null)

        is ChatRequest.Delivery.DeliveredByUsername ->
            LocalDelivery(ChatAcceptanceLocal.DeliveryStatus.DELIVERED, ChatAcceptanceLocal.DeliveredVia.USERNAME, period.toLong())

        is ChatRequest.Delivery.DeliveredAnonymously ->
            LocalDelivery(ChatAcceptanceLocal.DeliveryStatus.DELIVERED, ChatAcceptanceLocal.DeliveredVia.NOTIFICATION, period.toLong())
    }

    private fun ChatAcceptanceLocal.toDomain(): ChatAcceptance {
        return ChatAcceptance(
            contactAccountId = contactAccountId.toDataByteArray(),
            requestId = requestId,
            messageId = messageId,
            acceptedAt = Instant.fromEpochMilliseconds(acceptedAt),
            delivery = deliveryDomain(),
        )
    }

    private fun ChatAcceptanceLocal.deliveryDomain(): ChatRequest.Delivery = when (deliveryStatus) {
        ChatAcceptanceLocal.DeliveryStatus.UNDELIVERED -> ChatRequest.Delivery.Undelivered
        ChatAcceptanceLocal.DeliveryStatus.FAILED -> ChatRequest.Delivery.Failed
        ChatAcceptanceLocal.DeliveryStatus.DELIVERED -> deliveredDomain()
    }

    private fun ChatAcceptanceLocal.deliveredDomain(): ChatRequest.Delivery {
        val period = lastDeliveredPeriod?.toUInt()

        return when (deliveredVia) {
            ChatAcceptanceLocal.DeliveredVia.NOTIFICATION -> ChatRequest.Delivery.DeliveredAnonymously(
                requireNotNull(period) { "Acceptance $messageId delivered via NOTIFICATION has no period" }
            )

            ChatAcceptanceLocal.DeliveredVia.USERNAME -> period?.let { ChatRequest.Delivery.DeliveredByUsername(it) } ?: ChatRequest.Delivery.DeliveredUntracked

            null -> ChatRequest.Delivery.DeliveredUntracked
        }
    }
}
