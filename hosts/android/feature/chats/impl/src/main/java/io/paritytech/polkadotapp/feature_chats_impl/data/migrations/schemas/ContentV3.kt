package io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageReactionContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.OfferPurpose
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.RichTextContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.TokenPlatformLocal
import kotlinx.serialization.Serializable

/**
 * Content schema V3 — removed Media
 */
@Serializable
sealed class ContentV3 {
    @Serializable
    @EnumIndex(0)
    class Text(val text: String) : ContentV3()

    @Serializable
    @EnumIndex(1)
    class Token(val token: ByteArray, val platform: TokenPlatformLocal) : ContentV3()

    @Serializable
    @EnumIndex(3)
    @Deprecated("Contact Added is deprecated in favour of chat requests")
    object ContactAdded : ContentV3()

    @Serializable
    @EnumIndex(4)
    class Reacted(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ContentV3()

    @Serializable
    @EnumIndex(5)
    class ReactionRemoved(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ContentV3()

    @Serializable
    @EnumIndex(7)
    class Unsupported(val rawContent: ByteArray) : ContentV3()

    @Serializable
    @EnumIndex(9)
    object LeftChat : ContentV3()

    @Serializable
    @EnumIndex(10)
    class Edited(
        val messageId: String,
        val newText: String
    ) : ContentV3()

    @Serializable
    @EnumIndex(11)
    class DataChannelOffer(
        val sdp: ByteArray,
        val purpose: OfferPurpose
    ) : ContentV3()

    @Serializable
    @EnumIndex(12)
    class DataChannelAnswer(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ContentV3()

    @Serializable
    @EnumIndex(13)
    class DataChannelIceCandidate(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ContentV3()

    @Serializable
    @EnumIndex(14)
    class ChatAccepted(
        val requestId: String
    ) : ContentV3()

    @Serializable
    @EnumIndex(15)
    class ChatRequest(
        val welcome: RichTextContentLocal?
    ) : ContentV3()

    @Serializable
    @EnumIndex(16)
    class CoinagePayment(
        val totalValue: Balance,
        val coinKeys: List<ByteArraySerializable>,
        val status: LegacyCoinagePaymentStatusLocal,
    ) : ContentV3()

    @Serializable
    @EnumIndex(17)
    class DataChannelClosed(
        val offerMessageId: String
    ) : ContentV3()

    @Serializable
    @EnumIndex(18)
    class RichText(val content: RichTextContentLocal) : ContentV3()

    @Serializable
    @EnumIndex(255)
    class Custom(val rendererId: String, val rawContent: ByteArray?) : ContentV3()
}
