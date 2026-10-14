package io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageReactionContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.OfferPurpose
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.TokenPlatformLocal
import kotlinx.serialization.Serializable

/**
 * Content schema V1 — CoinagePayment does NOT have a `status` field.
 */
@Serializable
sealed class ContentV1 {
    @Serializable
    @EnumIndex(0)
    class Text(val text: String) : ContentV1()

    @Serializable
    @EnumIndex(1)
    class Token(val token: ByteArray, val platform: TokenPlatformLocal) : ContentV1()

    @Serializable
    @EnumIndex(3)
    object ContactAdded : ContentV1()

    @Serializable
    @EnumIndex(4)
    class Reacted(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ContentV1()

    @Serializable
    @EnumIndex(5)
    class ReactionRemoved(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ContentV1()

    @Serializable
    @EnumIndex(6)
    class Media(val url: String, val text: String?, val type: LegacyMultimediaType, val aspectRatio: LegacyAspectRatio) : ContentV1()

    @Serializable
    @EnumIndex(7)
    class Unsupported(val rawContent: ByteArray) : ContentV1()

    @Serializable
    @EnumIndex(8)
    class File(
        val url: String,
        val fileName: String,
        val mimeType: String,
        val sizeBytes: Long,
        val text: String?,
    ) : ContentV1()

    @Serializable
    @EnumIndex(9)
    object LeftChat : ContentV1()

    @Serializable
    @EnumIndex(10)
    class Edited(
        val messageId: String,
        val newText: String
    ) : ContentV1()

    @Serializable
    @EnumIndex(11)
    class DataChannelOffer(
        val sdp: ByteArray,
        val purpose: OfferPurpose
    ) : ContentV1()

    @Serializable
    @EnumIndex(12)
    class DataChannelAnswer(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ContentV1()

    @Serializable
    @EnumIndex(13)
    class DataChannelIceCandidate(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ContentV1()

    @Serializable
    @EnumIndex(14)
    class ChatAccepted(
        val requestId: String
    ) : ContentV1()

    @Serializable
    @EnumIndex(15)
    class ChatRequest(
        val welcome: LegacyRichTextContentLocal?
    ) : ContentV1()

    @Serializable
    @EnumIndex(16)
    class CoinagePayment(
        val totalValue: Balance,
        val coinKeys: List<ByteArraySerializable>,
    ) : ContentV1()

    @Serializable
    @EnumIndex(17)
    class DataChannelClosed(
        val offerMessageId: String
    ) : ContentV1()

    @Serializable
    @EnumIndex(255)
    class Custom(val rendererId: String, val rawContent: ByteArray?) : ContentV1()
}
