package io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.scale.X25519PublicKeyScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.RichTextContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenContent
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementProofRemote
import kotlinx.serialization.Serializable
import java.util.UUID

/**
 * Encrypted model sent over the network.
 * Preserves requester anonymity at transport layer.
 */
@Serializable
class ChatRequestEncryptedRemote(
    val encryptionPubKey: ByteArray,
    val encryptedRequest: ByteArray
)

/**
 * Decrypted request obtained by acceptor.
 */
@Serializable
class ChatRequestDecrypted(
    val message: ChatRequestMessage,
    val proof: StatementProofRemote
)

/**
 * Proof payload for verification.
 * The requester signs this payload to prove ownership.
 */
@Serializable
@Keep
class ChatRequestProofPayload(
    val message: ChatRequestMessage,
    val acceptor: ByteArray
)

@Serializable
class ChatRequestMessage(
    val messageId: String,
    val timestamp: ULong,
    val content: VersionedRequestContent
) {
    companion object {
        fun new(content: VersionedRequestContent): ChatRequestMessage {
            return ChatRequestMessage(
                messageId = UUID.randomUUID().toString(),
                timestamp = System.currentTimeMillis().toULong(),
                content = content
            )
        }
    }
}

@Serializable
sealed class VersionedRequestContent {
    @Serializable
    @EnumIndex(0)
    class V1(val content: RequestContentV1) : VersionedRequestContent() {
        companion object {
            fun new(
                pushToken: TokenContent?,
                welcomeMessage: RichTextContent?
            ): V1 {
                return V1(
                    RequestContentV1(
                        pushToken = pushToken,
                        welcomeMessage = welcomeMessage
                    )
                )
            }
        }
    }

    @Serializable
    @EnumIndex(1)
    class V2(val content: RequestContentV2) : VersionedRequestContent() {
        companion object {
            fun new(
                identityProof: IdentityProofScale,
                deviceEncPubKey: X25519PublicKeyScale,
                pushToken: TokenContent?,
                welcomeMessage: RichTextContent?
            ): V2 {
                return V2(
                    RequestContentV2(
                        identityProof = identityProof,
                        deviceEncPubKey = deviceEncPubKey,
                        pushToken = pushToken,
                        welcomeMessage = welcomeMessage
                    )
                )
            }
        }
    }
}

@Serializable
class RequestContentV1(
    val pushToken: TokenContent?,
    val welcomeMessage: RichTextContent?
)

@Serializable
class RequestContentV2(
    val identityProof: IdentityProofScale,
    val deviceEncPubKey: X25519PublicKeyScale,
    val pushToken: TokenContent?,
    val welcomeMessage: RichTextContent?
)

@Serializable
@Keep
class IdentityProofScale(
    @FixedLength(32)
    val identityAccountId: ByteArray,
    @FixedLength(32)
    val proof: ByteArray,
)

@Serializable
@Keep
class IdentityProofPayloadScale(
    @FixedLength(32)
    val identityAccountId: ByteArray,
    @FixedLength(32)
    val statementAccountId: ByteArray,
    val context: String,
)
