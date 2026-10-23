package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.Identifiable
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushToken
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import java.util.UUID

typealias ChatMessageId = String

data class ChatMessage(
    val id: ChatMessageId,
    val chatId: ChatId,
    val timestamp: Timestamp,
    val origin: ChatMessageOrigin,
    val content: Content,
    val status: Status,
    val replyToMessageId: String? = null
) : Identifiable {
    companion object {
        fun new(
            messageId: ChatMessageId = UUID.randomUUID().toString(),
            chatId: ChatId,
            content: Content,
            origin: ChatMessageOrigin,
            status: Status = Status.NEW,
            replyToMessageId: String? = null
        ): ChatMessage {
            val timestamp = System.currentTimeMillis()

            return ChatMessage(
                id = messageId,
                timestamp = timestamp,
                origin = origin,
                content = content,
                chatId = chatId,
                status = status,
                replyToMessageId = replyToMessageId
            )
        }
    }

    override val identifier = id

    sealed interface Content {
        class Text(val text: String) : Content
        class Token(val token: ChatPushToken, val operatingSystem: OperatingSystem, val isVoIP: Boolean = false) : Content
        class RichText(val text: String?, val attachments: List<Attachment>) : Content

        // TODO v1 RELEASE: remove as a part of v1 preparation
        @Deprecated("Contact Added is deprecated in favour of chat requests")
        object ContactAdded : Content
        object LeftChat : Content

        data class CoinagePayment(
            val totalValue: Balance,
            val coinKeys: List<ByteArray>,
            val status: Status
        ) : Content {
            sealed interface Status {
                data object Detecting : Status
                data class Detected(val amount: Balance) : Status

                /** Part of the payment is ours and the rest is still being claimed. */
                data class PartiallyClaimed(val claimed: Balance) : Status

                /** Claiming is over; [amount] is what landed, which may be less than was sent. */
                data class Transferred(val amount: Balance) : Status
                data object FailedDetection : Status
                data object FailedTransfer : Status
            }
        }

        class Reacted(val messageId: ChatMessageId, val content: ChatMessageReactionContent) : Content

        class ReactionRemoved(val messageId: ChatMessageId, val content: ChatMessageReactionContent) : Content

        class Edited(val messageId: ChatMessageId, val content: RichText) : Content

        class DataChannelOffer(val sdp: ByteArray, val purpose: Purpose) : Content {
            enum class Purpose {
                AUDIO_CALL, VIDEO_CALL
            }
        }

        class DataChannelAnswer(val offerMessageId: ChatMessageId, val sdp: ByteArray) : Content

        class DataChannelIceCandidate(val offerMessageId: ChatMessageId, val sdp: ByteArray) : Content

        class DataChannelClosed(val offerMessageId: ChatMessageId) : Content

        class Unsupported(val rawContent: ByteArray) : Content

        @Deprecated("Use DeviceChatAccepted instead")
        /**
         * Legacy chat request acceptance message (single device). Kept for parsing
         * incoming legacy messages; new outgoing acceptances use [DeviceChatAccepted].
         */
        data class ChatAccepted(val requestId: String) : Content

        /**
         * Chat request message containing an optional welcome message.
         */
        data class ChatRequest(val welcome: RichText?) : Content

        /**
         * Notifies a contact that a new device has been added to our onchain identity.
         * Carries the per-device statement-store account id and chat-message encryption public key.
         */
        data class DeviceAdded(
            val statementAccountId: AccountId,
            val encryptionPublicKey: X25519PublicKey,
        ) : Content

        /**
         * Notifies a contact that a device has been deregistered from our onchain identity.
         */
        data class DeviceRemoved(
            val statementAccountId: AccountId,
        ) : Content

        /**
         * Chat request acceptance carrying a single accepting device. Sent over an
         * identity-level session so all of the requester's devices can decrypt.
         */
        data class DeviceChatAccepted(
            val requestId: String,
            val device: DeviceInfo,
        ) : Content

        /**
         * Transport container referencing a HOP-uploaded batch of compacted messages.
         * Never displayed; the receiver claims the batch and expands it back into the originals.
         */
        data class CompactionCommit(
            val claimIdentifier: DataByteArray,
            val claimTicket: HopTicket,
            val nodeUrl: String,
        ) : Content

        /**
         * Terminal placeholder shown when a compacted batch is permanently unavailable
         * (gone from the pool and every fallback source) — the contained messages are lost.
         */
        object CompactionUnavailable : Content

        /**
         * Custom message content rendered by a [CustomChatMessageRenderer].
         *
         * @param rendererId The ID of the renderer that should handle this content
         * @param content The deserialized content result. Will be [Result.success] if parsing succeeded,
         *                or [Result.failure] if deserialization failed.
         */
        data class Custom<T>(val rendererId: String, val content: Result<T>) : Content

        /** A body of a product that the other kinds cannot hold (Chat Extension v2). */
        data class ProductBody(val body: ProductMessageBody) : Content

        /**
         * A call of a room of the chat product, as the product records it (`ChatCallRecord`). The time of the message is
         * the time of the offer and its origin is the caller. Times are milliseconds since the Unix epoch.
         */
        data class ContactCall(
            val video: Boolean,
            val answeredAt: Long?,
            val closedAt: Long?,
            /** True: the user ended or declined the call. */
            val closedByUser: Boolean,
        ) : Content
    }

    enum class Status(val deliveryRank: Int) {
        PROCESSING(0),
        NEW(1),
        IS_SENT(2),
        IS_READ(3),

        DELIVERY_FAILED(1)
    }
}

val ChatMessage.direction: ChatMessageDirection
    get() = when (origin) {
        is ChatMessageOrigin.Contact,
        is ChatMessageOrigin.Extension -> ChatMessageDirection.INCOMING

        is ChatMessageOrigin.User -> ChatMessageDirection.OUTGOING
    }

val ChatMessage.isIncoming
    get() = direction == ChatMessageDirection.INCOMING

fun ChatMessage.paymentContentOrNull(): Content.CoinagePayment? {
    return content as? Content.CoinagePayment
}

fun Content.isInternal(): Boolean {
    return this is Content.Token ||
        this is Content.DataChannelIceCandidate ||
        this is Content.DataChannelAnswer ||
        this is Content.DataChannelClosed ||
        this is Content.DeviceAdded ||
        this is Content.DeviceRemoved ||
        this is Content.CompactionCommit
}

inline fun <reified T> ChatMessage.customContentOrNull(): T? {
    val customContent = content as? Content.Custom<*> ?: return null
    return customContent.content.getOrNull() as? T
}

inline fun <reified T> List<ChatMessage>.filterCustomContents(): List<T> {
    return mapNotNull { it.customContentOrNull<T>() }
}
