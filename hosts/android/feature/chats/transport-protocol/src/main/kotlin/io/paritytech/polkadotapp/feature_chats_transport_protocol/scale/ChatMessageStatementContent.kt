package io.paritytech.polkadotapp.feature_chats_transport_protocol.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import kotlinx.serialization.Serializable

@Serializable
class ChatMessageStatement(
    val id: String,
    val timestamp: ULong,
    val versioned: VersionedChatMessage
)

@Serializable
sealed interface VersionedChatMessage {
    @Serializable
    @EnumIndex(0)
    class V1(val message: ChatMessageV1) : VersionedChatMessage
}

@Serializable
class ChatMessageV1(
    val content: ChatMessageStatementContent
)

@Serializable
sealed class ChatMessageStatementContent {
    @Serializable
    @EnumIndex(0)
    class Text(val text: String) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(1)
    class Token(val content: TokenContent) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(3)
    object ContactAdded : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(4)
    class Reacted(
        val messageId: String,
        val content: ChatMessageReactionStatementContent,
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(5)
    class ReactionRemoved(
        val messageId: String,
        val content: ChatMessageReactionStatementContent,
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(7)
    class Reply(
        val messageId: String,
        val ownContent: RichTextContent
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(8)
    class DataChannelOffer(
        val sdp: ByteArray,
        val purpose: DataChannelPurpose
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(9)
    class DataChannelAnswer(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(10)
    class DataChannelIceCandidate(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(11)
    class DataChannelClosed(
        val offerMessageId: String
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(12)
    class Edited(
        val messageId: String,
        val newContent: RichTextContent
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(13)
    object LeftChat : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(14)
    class ChatAccepted(
        val requestId: String
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(15)
    class RichText(val content: RichTextContent) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(16)
    class CoinagePayment(
        val totalValue: Balance,
        val coinKeys: List<ByteArraySerializable>
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(17)
    class DeviceAdded(
        val statementAccountId: AccountId,
        val encryptionPublicKey: EncodedPublicKey
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(18)
    class DeviceRemoved(
        val statementAccountId: AccountId
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(19)
    class CompactionCommit(
        val claimIdentifier: ByteArraySerializable,
        val claimTicket: ByteArraySerializable,
        val node: NodeEndpoint
    ) : ChatMessageStatementContent()

    @Serializable
    @EnumIndex(20)
    class DeviceChatAccepted(
        val requestId: String,
        val device: DeviceInfoScale,
    ) : ChatMessageStatementContent()
}

@Serializable
enum class TokenPlatform {
    ANDROID, IOS, IOS_VOIP
}

@JvmInline
@Serializable
value class ChatMessageReactionStatementContent(val emoji: String)

@Serializable
class AlwaysDecodableChatMessagePart(
    val id: String,
    val timestamp: ULong,
)

@Serializable
class RichTextContent(
    val text: String?,
    val attachments: List<FileVariant>?
)

@Serializable
class TokenContent(
    val token: ByteArray,
    val platform: TokenPlatform
)

@Serializable
sealed interface FileVariant {
    @Serializable
    @EnumIndex(0)
    class P2PMixnet(val file: P2PMixnetFile) : FileVariant
}

@Serializable
class P2PMixnetFile(
    val identifier: ByteArraySerializable,
    val claimTicket: ByteArraySerializable,
    val nodeEndpoint: NodeEndpoint,
    val meta: FileMeta
)

@Serializable
sealed interface FileMeta {
    @Serializable
    @EnumIndex(0)
    class General(val general: GeneralFileMeta) : FileMeta

    @Serializable
    @EnumIndex(1)
    class Image(val image: ImageFileMeta) : FileMeta

    @Serializable
    @EnumIndex(2)
    class Video(val video: VideoFileMeta) : FileMeta
}

@Serializable
class GeneralFileMeta(
    val mimeType: String,
    val fileSize: UInt
)

@Serializable
class ImageFileMeta(
    val general: GeneralFileMeta,
    val width: UInt,
    val height: UInt,
    val thumbnail: MediaThumbnail?
)

@Serializable
class VideoFileMeta(
    val general: GeneralFileMeta,
    val duration: UInt,
    val thumbnail: MediaThumbnail?
)

@Serializable
enum class DataChannelPurpose {
    AUDIO_CALL, VIDEO_CALL
}

@Serializable
sealed class NodeEndpoint {
    @Serializable
    @EnumIndex(0)
    class WssUrl(val url: String) : NodeEndpoint()
}

typealias MediaThumbnail = ByteArray
