package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.WithLength32
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductAccountIdScale
import kotlinx.serialization.Serializable

/** SCALE wire form of a plain 32-byte account id (`AccountId = Bytes(32)` on the JS side). */
typealias AccountIdScale = WithLength32<ByteArray>

@Serializable
class SsoEncodedTransactionExtensionValueScale(
    val id: String,
    val explicit: ByteArray,
    val implicit: ByteArray,
)

/**
 * SCALE tx payload, generic over the signer. The product flow uses [ProductAccountIdScale]; the
 * legacy flow uses a plain 32-byte account id ([AccountIdScale]), mirroring
 * `GenericTxPayloadV1(Signer)` on the JS host-papp side.
 */
@Serializable
class SsoTxPayloadScale<Signer>(
    val signer: Signer,
    @FixedLength(32) val genesisHash: ByteArray,
    val callData: ByteArray,
    val extensions: List<SsoEncodedTransactionExtensionValueScale>,
    val txExtVersion: UByte,
)

@Serializable
sealed interface SsoVersionedTxPayloadScale {
    @Serializable
    @EnumIndex(0)
    class V1(val payload: SsoTxPayloadScale<ProductAccountIdScale>) : SsoVersionedTxPayloadScale
}

@Serializable
class SsoCreateTransactionRequestScale(val payload: SsoVersionedTxPayloadScale)

@Serializable
sealed interface SsoVersionedLegacyTxPayloadScale {
    @Serializable
    @EnumIndex(0)
    class V1(val payload: SsoTxPayloadScale<AccountIdScale>) : SsoVersionedLegacyTxPayloadScale
}

@Serializable
class SsoCreateTransactionLegacyRequestScale(val payload: SsoVersionedLegacyTxPayloadScale)
