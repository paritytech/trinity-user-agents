package io.paritytech.polkadotapp.feature_statement_store_api.data.models

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import kotlinx.serialization.Serializable

@Serializable
sealed class StatementProofRemote {
    @Serializable
    @EnumIndex(0)
    class Sr25519(
        @FixedLength(64) val signature: ByteArray,
        @FixedLength(32) val signer: ByteArray,
    ) : StatementProofRemote()
}
