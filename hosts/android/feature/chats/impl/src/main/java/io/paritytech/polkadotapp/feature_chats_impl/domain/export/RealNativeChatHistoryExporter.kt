package io.paritytech.polkadotapp.feature_chats_impl.domain.export

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_impl.data.migration.NativeChatReader
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toEncodedMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.AlwaysFailCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.IncomingStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.OutgoingStatusScale
import javax.inject.Inject
import kotlin.time.Instant

class RealNativeChatHistoryExporter @Inject constructor(
    private val reader: NativeChatReader,
    private val chatMessageRepository: ChatMessageRepository,
) : NativeChatHistoryExporter {
    override suspend fun export(): ByteArray? {
        val contacts = reader.contacts().takeIf { it.isNotEmpty() } ?: return null
        val export = HistoryExportScale.V1(
            contacts = contacts.map { it.toScale() },
            requests = contacts.mapNotNull { contact ->
                contact.pendingChatRequestId?.let { reader.request(it) }?.toScale(contact)
            },
            messages = contactMessages().map { it.toScale() },
        )
        return BinaryScale.encodeToByteArray(HistoryExportScale.serializer(), export)
    }

    /** The stored messages of the contact chats, in the wire form that native device sync sent them in. */
    private suspend fun contactMessages(): List<NativeMessage> =
        chatMessageRepository.getMessagesUpdatedAfter(Instant.DISTANT_PAST, AlwaysFailCustomContentDecoder()).mapNotNull { message ->
            val contact = message.chatId.contactOrNull() ?: return@mapNotNull null
            // A request is kept as its welcome message.
            val wire = when (val content = message.content) {
                is ChatMessage.Content.ChatRequest -> content.welcome?.let { message.copy(content = it) } ?: return@mapNotNull null
                else -> message
            }
            wire.toEncodedMessage()
                .logFailure("The native message ${message.id} could not be exported")
                .map { encoded ->
                    NativeMessage(
                        encoded = encoded,
                        peerAccountId = contact.contactAccountId,
                        isOutgoing = wire.origin is ChatMessageOrigin.User,
                        status = wire.status,
                        timestamp = wire.timestamp,
                    )
                }
                .getOrNull()
        }

    private class NativeMessage(
        val encoded: ByteArray,
        val peerAccountId: AccountId,
        val isOutgoing: Boolean,
        val status: ChatMessage.Status,
        val timestamp: Long,
    )

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
        devices = reader.devices(accountId).map {
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

    private fun NativeMessage.toScale() = HistoryMessageScale(
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
