package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.handshake

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.scale.X25519PublicKeyScale
import kotlinx.serialization.Serializable

@Serializable
sealed interface VersionedHandshakeAnswerScale {
    @Serializable
    @EnumIndex(1)
    class V2(val value: HandshakeAnswerV2Scale) : VersionedHandshakeAnswerScale
}

@Serializable
@Keep
class HandshakeAnswerV2Scale(
    val encryptedData: ByteArray,
    val tempSharedEncryptionPublicKey: X25519PublicKeyScale,
)

@Serializable
sealed interface EncryptedHandshakeResponseV2Scale {
    @Serializable
    @EnumIndex(0)
    class Pending(val status: HandshakeStatusV2Scale) : EncryptedHandshakeResponseV2Scale

    @Serializable
    @EnumIndex(1)
    class Success(val data: HandshakeSuccessV2Scale) : EncryptedHandshakeResponseV2Scale

    @Serializable
    @EnumIndex(2)
    class Failed(val reason: String) : EncryptedHandshakeResponseV2Scale
}

@Serializable
sealed interface HandshakeStatusV2Scale {
    @Serializable
    @EnumIndex(0)
    data object AllowanceAllocation : HandshakeStatusV2Scale
}

@Serializable
@Keep
class HandshakeSuccessV2Scale(
    @FixedLength(32)
    val identityAccountId: ByteArray,
    @FixedLength(32)
    val rootAccountId: ByteArray,
    @FixedLength(32)
    val identityChatPrivateKey: ByteArray,
    val ssoEncrPubKey: X25519PublicKeyScale,
    val deviceEncPubKey: X25519PublicKeyScale,
    @FixedLength(32)
    val rootEntropySource: ByteArray,
)
