package io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import kotlinx.serialization.Serializable

/**
 * Any change to this class or its nested types requires a content migration.
 * See [io.paritytech.polkadotapp.database.migrations.ChatMessageContentMigration] for instructions.
 */
@Serializable
sealed class ChatMessageContentLocal {
    @Serializable
    @EnumIndex(0)
    class Text(val text: String) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(1)
    class Token(val token: ByteArray, val platform: TokenPlatformLocal) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(3)
    @Deprecated("Contact Added is deprecated in favour of chat requests")
    object ContactAdded : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(4)
    class Reacted(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(5)
    class ReactionRemoved(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(7)
    class Unsupported(val rawContent: ByteArray) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(9)
    object LeftChat : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(10)
    class Edited(
        val messageId: String,
        val richTextContent: RichTextContentLocal
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(11)
    class DataChannelOffer(
        val sdp: ByteArray,
        val purpose: OfferPurpose
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(12)
    class DataChannelAnswer(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(13)
    class DataChannelIceCandidate(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(14)
    class ChatAccepted(
        val requestId: String
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(15)
    class ChatRequest(
        val welcome: RichTextContentLocal?
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(16)
    class CoinagePayment(
        val totalValue: Balance,
        val coinKeys: List<ByteArraySerializable>,
        val status: CoinagePaymentStatusLocal,
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(17)
    class DataChannelClosed(
        val offerMessageId: String
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(18)
    class RichText(val content: RichTextContentLocal) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(19)
    class DeviceAdded(
        val statementAccountId: AccountId,
        val encryptionPublicKey: EncodedPublicKey,
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(20)
    class DeviceRemoved(
        val statementAccountId: AccountId,
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(21)
    class DeviceChatAccepted(
        val requestId: String,
        val device: DeviceInfoLocal,
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(22)
    class CompactionCommit(
        val claimIdentifier: ByteArraySerializable,
        val claimTicket: ByteArraySerializable,
        val nodeUrl: String,
    ) : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(23)
    object CompactionUnavailable : ChatMessageContentLocal()

    @Serializable
    @EnumIndex(255)
    class Custom(val rendererId: String, val rawContent: ByteArray?) : ChatMessageContentLocal()
}

@Serializable
class DeviceInfoLocal(
    val statementAccountId: AccountId,
    val encryptionPublicKey: EncodedPublicKey,
)

@Serializable
enum class TokenPlatformLocal {
    ANDROID, IOS
}

@Serializable
enum class OfferPurpose {
    AUDIO_CALL, VIDEO_CALL
}

@Serializable
sealed interface CoinagePaymentStatusLocal {
    @Serializable
    @EnumIndex(0)
    object Detecting : CoinagePaymentStatusLocal

    @Serializable
    @EnumIndex(1)
    class Detected(val detected: Balance) : CoinagePaymentStatusLocal

    @Serializable
    @EnumIndex(2)
    class Transferred(val transferred: Balance) : CoinagePaymentStatusLocal

    @Serializable
    @EnumIndex(3)
    object FailedDetection : CoinagePaymentStatusLocal

    @Serializable
    @EnumIndex(4)
    object FailedTransfer : CoinagePaymentStatusLocal

    @Serializable
    @EnumIndex(5)
    class PartiallyClaimed(val claimed: Balance) : CoinagePaymentStatusLocal
}

@JvmInline
@Serializable
value class ChatMessageReactionContentLocal(val emoji: String)

@Serializable
class RichTextContentLocal(
    val text: String?,
    val attachments: List<AttachmentLocal>?
)

@Serializable
sealed interface AttachmentLocal {
    @Serializable
    @EnumIndex(0)
    class Embedded(
        val uri: String,
        val meta: AttachmentMetaLocal
    ) : AttachmentLocal

    @Serializable
    @EnumIndex(1)
    class Hosted(
        val uri: String?,
        val meta: AttachmentMetaLocal,
        val identifier: ByteArraySerializable,
        val claimTicket: ByteArraySerializable,
        val nodeUrl: String
    ) : AttachmentLocal
}

@Serializable
sealed interface AttachmentMetaLocal {
    @Serializable
    @EnumIndex(0)
    class Image(
        val width: Int,
        val height: Int,
        val mimeType: String,
        val sizeBytes: Long,
        val blurHash: String?
    ) : AttachmentMetaLocal

    @Serializable
    @EnumIndex(1)
    class Video(
        val duration: Long,
        val mimeType: String,
        val sizeBytes: Long,
        val blurHash: String?
    ) : AttachmentMetaLocal

    @Serializable
    @EnumIndex(2)
    class General(val fileName: String, val mimeType: String, val sizeBytes: Long) : AttachmentMetaLocal
}
