package io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageReactionContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.DeviceInfoLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.OfferPurpose
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.RichTextContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.TokenPlatformLocal
import kotlinx.serialization.Serializable

/**
 * Content schema V4 — CoinagePayment.status is the legacy plain enum [LegacyCoinagePaymentStatusLocal].
 * Replaced by the current shape whose status is a sealed type with Detected carrying a Balance.
 */
@Serializable
sealed class ContentV4 {
    @Serializable
    @EnumIndex(0)
    class Text(val text: String) : ContentV4()

    @Serializable
    @EnumIndex(1)
    class Token(val token: ByteArray, val platform: TokenPlatformLocal) : ContentV4()

    @Serializable
    @EnumIndex(3)
    @Deprecated("Contact Added is deprecated in favour of chat requests")
    object ContactAdded : ContentV4()

    @Serializable
    @EnumIndex(4)
    class Reacted(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ContentV4()

    @Serializable
    @EnumIndex(5)
    class ReactionRemoved(
        val messageId: String,
        val content: ChatMessageReactionContentLocal,
    ) : ContentV4()

    @Serializable
    @EnumIndex(7)
    class Unsupported(val rawContent: ByteArray) : ContentV4()

    @Serializable
    @EnumIndex(9)
    object LeftChat : ContentV4()

    @Serializable
    @EnumIndex(10)
    class Edited(
        val messageId: String,
        val richTextContent: RichTextContentLocal
    ) : ContentV4()

    @Serializable
    @EnumIndex(11)
    class DataChannelOffer(
        val sdp: ByteArray,
        val purpose: OfferPurpose
    ) : ContentV4()

    @Serializable
    @EnumIndex(12)
    class DataChannelAnswer(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ContentV4()

    @Serializable
    @EnumIndex(13)
    class DataChannelIceCandidate(
        val offerMessageId: String,
        val sdp: ByteArray
    ) : ContentV4()

    @Serializable
    @EnumIndex(14)
    class ChatAccepted(
        val requestId: String
    ) : ContentV4()

    @Serializable
    @EnumIndex(15)
    class ChatRequest(
        val welcome: RichTextContentLocal?
    ) : ContentV4()

    @Serializable
    @EnumIndex(16)
    class CoinagePayment(
        val totalValue: Balance,
        val coinKeys: List<ByteArraySerializable>,
        val status: LegacyCoinagePaymentStatusLocal
    ) : ContentV4()

    @Serializable
    @EnumIndex(17)
    class DataChannelClosed(
        val offerMessageId: String
    ) : ContentV4()

    @Serializable
    @EnumIndex(18)
    class RichText(val content: RichTextContentLocal) : ContentV4()

    @Serializable
    @EnumIndex(19)
    class DeviceAdded(
        val statementAccountId: AccountId,
        val encryptionPublicKey: EncodedPublicKey
    ) : ContentV4()

    @Serializable
    @EnumIndex(20)
    class DeviceRemoved(
        val statementAccountId: AccountId
    ) : ContentV4()

    @Serializable
    @EnumIndex(21)
    class DeviceChatAccepted(
        val requestId: String,
        val device: DeviceInfoLocal
    ) : ContentV4()

    @Serializable
    @EnumIndex(255)
    class Custom(val rendererId: String, val rawContent: ByteArray?) : ContentV4()
}
