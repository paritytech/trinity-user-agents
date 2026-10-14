package io.paritytech.polkadotapp.feature_chats_impl.data.hop.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import kotlinx.serialization.Serializable

@Serializable
sealed class HopMultiSigner {
    @Serializable
    @EnumIndex(0)
    class Ed25519(@FixedLength(32) val publicKey: ByteArray) : HopMultiSigner()

    @Serializable
    @EnumIndex(1)
    class SR25519(@FixedLength(32) val publicKey: ByteArray) : HopMultiSigner()

    @Serializable
    @EnumIndex(2)
    class ECDSA(@FixedLength(33) val publicKey: ByteArray) : HopMultiSigner()
}

@Serializable
sealed class HopMultiSignature {
    @Serializable
    @EnumIndex(0)
    class Ed25519(@FixedLength(64) val signature: ByteArray) : HopMultiSignature()

    @Serializable
    @EnumIndex(1)
    class SR25519(@FixedLength(64) val signature: ByteArray) : HopMultiSignature()

    @Serializable
    @EnumIndex(2)
    class ECDSA(@FixedLength(65) val signature: ByteArray) : HopMultiSignature()
}
