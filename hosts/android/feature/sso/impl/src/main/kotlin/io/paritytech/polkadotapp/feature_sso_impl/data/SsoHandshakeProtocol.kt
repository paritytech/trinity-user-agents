package io.paritytech.polkadotapp.feature_sso_impl.data

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.common.domain.model.EncodedPrivateKey
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.scale.toDomain
import io.paritytech.polkadotapp.common.domain.model.scale.toScale
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeDevice
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeMetadata
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeOffer
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.EncryptedHandshakeResponseV2Scale
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.HandshakeAnswerV2Scale
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.HandshakeProposalV2Scale
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.HandshakeStatusV2Scale
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.HandshakeSuccessV2Scale
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.MetadataKeyScale
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.VersionedHandshakeAnswerScale
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake.VersionedHandshakeOfferScale
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.HandshakeAnswer
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementData
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import kotlinx.serialization.decodeFromByteArray
import javax.inject.Inject

interface SsoHandshakeProtocol {
    fun isPairingDeeplink(deeplink: Uri): Boolean

    fun parsePairDeeplink(deeplink: Uri): Result<HandshakeOffer>

    fun encodeResponse(response: HandshakeProtocolResponse): ByteArray

    fun encodeAnswerStatementData(answer: HandshakeAnswer): StatementData

    fun statementTopic(
        hostStatementStorePublicKey: EncodedPublicKey,
        hostSharedSecretPublicKey: X25519PublicKey,
    ): ByteArray

    fun statementChannel(
        hostStatementStorePublicKey: EncodedPublicKey,
        hostSharedSecretPublicKey: X25519PublicKey,
    ): ByteArray

    fun statementExpiry(): ULong
}

sealed interface HandshakeProtocolResponse {
    data object AllowanceAllocation : HandshakeProtocolResponse
    data class Success(val payload: HandshakeSuccessPayload) : HandshakeProtocolResponse
    data class Failure(val reason: String) : HandshakeProtocolResponse
}

class HandshakeSuccessPayload(
    val identityAccountId: ByteArray,
    val rootAccountId: ByteArray,
    val identityChatPrivateKey: EncodedPrivateKey,
    val ssoEncrPubKey: X25519PublicKey,
    val deviceEncPubKey: X25519PublicKey,
    // Per TrUAPI RFC-7 layer 1: blake2b256_keyed(rootAccountSecret, "product-entropy-derivation").
    // The host caches this and uses it as the seed for every product-scoped entropy derivation.
    val rootEntropySource: ByteArray,
)

class RealSsoHandshakeProtocol @Inject constructor() : SsoHandshakeProtocol {
    private companion object {
        private const val PAIR_HOST = "pair"
        private const val HANDSHAKE_PARAM = "handshake"

        private val TOPIC_PREIMAGE_SUFFIX = "topic".encodeToByteArray()
        private val CHANNEL_PREIMAGE_SUFFIX = "channel".encodeToByteArray()
    }

    override fun isPairingDeeplink(deeplink: Uri): Boolean {
        return deeplink.scheme == DeepLinkHandler.APP_SCHEME && deeplink.host == PAIR_HOST
    }

    override fun parsePairDeeplink(deeplink: Uri): Result<HandshakeOffer> {
        val handshakeHex = deeplink.getQueryParameter(HANDSHAKE_PARAM)
            ?: return Result.failure(IllegalArgumentException("Missing '$HANDSHAKE_PARAM' parameter in pair deeplink"))

        return runCatching {
            BinaryScale.decodeFromByteArray<VersionedHandshakeOfferScale>(handshakeHex.fromHex())
                .toDomain()
        }
    }

    override fun encodeResponse(response: HandshakeProtocolResponse): ByteArray {
        val payload: EncryptedHandshakeResponseV2Scale = when (response) {
            is HandshakeProtocolResponse.AllowanceAllocation ->
                EncryptedHandshakeResponseV2Scale.Pending(HandshakeStatusV2Scale.AllowanceAllocation)

            is HandshakeProtocolResponse.Success -> EncryptedHandshakeResponseV2Scale.Success(
                HandshakeSuccessV2Scale(
                    identityAccountId = response.payload.identityAccountId,
                    rootAccountId = response.payload.rootAccountId,
                    identityChatPrivateKey = response.payload.identityChatPrivateKey.value,
                    ssoEncrPubKey = response.payload.ssoEncrPubKey.toScale(),
                    deviceEncPubKey = response.payload.deviceEncPubKey.toScale(),
                    rootEntropySource = response.payload.rootEntropySource,
                )
            )

            is HandshakeProtocolResponse.Failure -> EncryptedHandshakeResponseV2Scale.Failed(response.reason)
        }
        return payload.scaleEncodeBinary()
    }

    override fun encodeAnswerStatementData(answer: HandshakeAnswer): ByteArray {
        return answer.toScale().scaleEncodeBinary()
    }

    override fun statementTopic(
        hostStatementStorePublicKey: EncodedPublicKey,
        hostSharedSecretPublicKey: X25519PublicKey
    ): ByteArray {
        val toHash = hostSharedSecretPublicKey.bytes.value + TOPIC_PREIMAGE_SUFFIX
        return toHash.blake2b256(hostStatementStorePublicKey.value)
    }

    override fun statementChannel(
        hostStatementStorePublicKey: EncodedPublicKey,
        hostSharedSecretPublicKey: X25519PublicKey
    ): ByteArray {
        val toHash = hostSharedSecretPublicKey.bytes.value + CHANNEL_PREIMAGE_SUFFIX
        return toHash.blake2b256(hostStatementStorePublicKey.value)
    }

    override fun statementExpiry(): ULong {
        return StatementExpiry.createForCurrentTimestamp()
    }

    private fun HandshakeAnswer.toScale(): VersionedHandshakeAnswerScale {
        return VersionedHandshakeAnswerScale.V2(
            HandshakeAnswerV2Scale(
                encryptedData = encryptedData,
                tempSharedEncryptionPublicKey = tempSharedEncryptionPublicKey.toScale()
            )
        )
    }

    private fun VersionedHandshakeOfferScale.toDomain(): HandshakeOffer {
        return when (this) {
            is VersionedHandshakeOfferScale.V2 -> value.toDomain()
        }
    }

    private fun HandshakeProposalV2Scale.toDomain(): HandshakeOffer {
        return HandshakeOffer(
            device = HandshakeDevice(
                statementAccountId = device.statementAccountId.intoAccountId(),
                encryptionPublicKey = device.encryptionPublicKey.toDomain().getOrThrow(),
            ),
            metadata = HandshakeMetadata(
                entries = metadata.associate { it.key.toDomain() to it.value }
            ),
        )
    }

    private fun MetadataKeyScale.toDomain(): HandshakeMetadata.Key = when (this) {
        is MetadataKeyScale.Custom -> HandshakeMetadata.Key.Custom(name)
        MetadataKeyScale.HostName -> HandshakeMetadata.Key.HostName
        MetadataKeyScale.HostVersion -> HandshakeMetadata.Key.HostVersion
        MetadataKeyScale.HostIcon -> HandshakeMetadata.Key.HostIcon
        MetadataKeyScale.PlatformType -> HandshakeMetadata.Key.PlatformType
        MetadataKeyScale.PlatformVersion -> HandshakeMetadata.Key.PlatformVersion
        MetadataKeyScale.Location -> HandshakeMetadata.Key.Location
    }
}
