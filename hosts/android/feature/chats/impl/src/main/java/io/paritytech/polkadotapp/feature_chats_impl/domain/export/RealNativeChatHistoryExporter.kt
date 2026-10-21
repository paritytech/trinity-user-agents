package io.paritytech.polkadotapp.feature_chats_impl.domain.export

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.ApplyRemoteChatMessageUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactDevicesRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.IncomingStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.OutgoingStatusScale
import javax.inject.Inject
import kotlin.time.Instant

class RealNativeChatHistoryExporter @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val contactDevicesRepository: ContactDevicesRepository,
    private val chatRequestRepository: ChatRequestRepository,
    private val messages: ApplyRemoteChatMessageUseCase,
) : NativeChatHistoryExporter {
    override suspend fun export(): ByteArray? {
        val contacts = contactsRepository.getContacts().takeIf { it.isNotEmpty() } ?: return null
        val export = HistoryExportScale.V1(
            contacts = contacts.map { it.toScale() },
            requests = contacts.mapNotNull { contact ->
                contact.pendingChatRequestId?.let { chatRequestRepository.getById(it) }?.toScale(contact)
            },
            messages = messages.getMessagesUpdatedAfter(Instant.DISTANT_PAST).map { it.toScale() },
        )
        return BinaryScale.encodeToByteArray(HistoryExportScale.serializer(), export)
    }

    private suspend fun Contact.toScale() = HistoryContactScale(
        accountId = accountId.value,
        username = username,
        chatKey = chatKey.bytes.value,
        pushToken = pushToken?.value,
        pushPlatform = when (operatingSystem) {
            OperatingSystem.ANDROID -> TOKEN_PLATFORM_ANDROID
            OperatingSystem.IOS -> TOKEN_PLATFORM_IOS
            OperatingSystem.UNKNOWN -> null
        },
        voipToken = voipPushToken?.value,
        blocked = isBlocked,
        peerLeft = isPeerLeft,
        addedAt = addedAt.toEpochMilliseconds().toULong(),
        establishedAt = establishedAt?.toEpochMilliseconds()?.toULong(),
        devices = contactDevicesRepository.getDevices(accountId).map {
            HistoryDeviceScale(it.statementAccountId.value, it.encryptionPublicKey.bytes.value)
        },
    )

    private fun ChatRequest.toScale(contact: Contact) = HistoryRequestScale(
        id = id,
        peer = contact.accountId.value,
        timestamp = timestamp.toULong(),
        outgoing = direction == ChatRequest.Direction.OUTGOING,
        status = when (status) {
            ChatRequest.Status.PENDING -> 0u
            ChatRequest.Status.ACCEPTED -> 1u
            ChatRequest.Status.DECLINED -> 2u
        },
        delivered = delivery is ChatRequest.Delivery.Delivered || delivery is ChatRequest.Delivery.DeliveredAnonymously,
        welcome = null,
    )

    private fun ApplyRemoteChatMessageUseCase.MessageForSync.toScale() = HistoryMessageScale(
        peerId = peerAccountId.value,
        message = encoded,
        status = if (isOutgoing) LocalStatusScale.Outgoing(status.toOutgoing()) else LocalStatusScale.Incoming(status.toIncoming()),
        order = timestamp.toULong(),
    )

    // As device sync maps them (`SyncEntityCollector`).
    private fun ChatMessage.Status.toOutgoing(): OutgoingStatusScale = when (this) {
        ChatMessage.Status.PROCESSING, ChatMessage.Status.DELIVERY_FAILED -> OutgoingStatusScale.NEW
        ChatMessage.Status.NEW, ChatMessage.Status.IS_SENT -> OutgoingStatusScale.SENT
        ChatMessage.Status.IS_READ -> OutgoingStatusScale.DELIVERED
    }

    private fun ChatMessage.Status.toIncoming(): IncomingStatusScale =
        if (this == ChatMessage.Status.IS_READ) IncomingStatusScale.SEEN else IncomingStatusScale.NEW

    private companion object {
        const val TOKEN_PLATFORM_ANDROID: UByte = 0u
        const val TOKEN_PLATFORM_IOS: UByte = 1u
    }
}
