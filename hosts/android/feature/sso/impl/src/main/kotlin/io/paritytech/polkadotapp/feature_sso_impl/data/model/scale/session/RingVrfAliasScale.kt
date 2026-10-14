package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfKeyDisclosure
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductAccountIdScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductDerivationIndexScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductIdScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.RingLocationScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.toDomain
import io.paritytech.polkadotapp.feature_products_api.model.scale.toScale
import kotlinx.serialization.Serializable

@Serializable
@AsTuple
data class ProductProofContextScale(
    val productId: ProductIdScale,
    val suffix: ProductDerivationIndexScale,
)

// RFC-0004 shared failure set for ring-VRF alias and proof responses over the SSO channel;
// mirrors the Account Holder's RingVrfError. Variant order is the cross-host wire contract.
@Serializable
sealed class SsoRingVrfErrorScale {
    @Serializable
    @EnumIndex(0)
    data object RingNotFound : SsoRingVrfErrorScale()

    @Serializable
    @EnumIndex(1)
    data object NotMember : SsoRingVrfErrorScale()

    // RFC-0024 inserts the key-handle failures before Rejected, shifting Rejected and Unknown.
    @Serializable
    @EnumIndex(2)
    data object KeyNotRegistered : SsoRingVrfErrorScale()

    @Serializable
    @EnumIndex(3)
    data object KeyNotInRing : SsoRingVrfErrorScale()

    @Serializable
    @EnumIndex(4)
    data object NotAllowlisted : SsoRingVrfErrorScale()

    @Serializable
    @EnumIndex(5)
    data object Rejected : SsoRingVrfErrorScale()

    @Serializable
    @EnumIndex(6)
    data class Unknown(val reason: String) : SsoRingVrfErrorScale()
}

@Serializable
sealed class RingVrfKeyDisclosureScale {
    @Serializable
    @EnumIndex(0)
    data object Anonymized : RingVrfKeyDisclosureScale()

    @Serializable
    @EnumIndex(1)
    data object PublicKey : RingVrfKeyDisclosureScale()
}

// RFC-0024 registry entry. publicKey is Option<[u8; 32]> — present only when the caller owns the key
// or holds a public-key disclosure grant.
@Serializable
data class RegisteredRingVrfKeyScale(
    val handle: ProductAccountIdScale,
    val rings: List<RingLocationScale>,
    val publicKey: DataByteArray?,
)

// RFC-0004 create_proof result carried in a RingVrfProofResponse. ringIndex/ringRevision are u32.
@Serializable
data class SsoRingVrfProofScale(
    val proof: DataByteArray,
    val contextualAlias: SsoContextualAliasScale,
    val ringIndex: Int,
    val ringRevision: Int,
)

fun ProductProofContext.toScale(): ProductProofContextScale =
    ProductProofContextScale(productId.value, suffix.toScale())

fun ProductProofContextScale.toDomain(): Result<ProductProofContext> =
    suffix.toDomain().map { ProductProofContext(ProductId.fromStoredValue(productId), it) }

fun RingVrfKeyDisclosure.toScale(): RingVrfKeyDisclosureScale = when (this) {
    RingVrfKeyDisclosure.ANONYMIZED -> RingVrfKeyDisclosureScale.Anonymized
    RingVrfKeyDisclosure.PUBLIC_KEY -> RingVrfKeyDisclosureScale.PublicKey
}

fun RingVrfKeyDisclosureScale.toDomain(): RingVrfKeyDisclosure = when (this) {
    RingVrfKeyDisclosureScale.Anonymized -> RingVrfKeyDisclosure.ANONYMIZED
    RingVrfKeyDisclosureScale.PublicKey -> RingVrfKeyDisclosure.PUBLIC_KEY
}

fun RegisteredRingVrfKey.toScale(): RegisteredRingVrfKeyScale =
    RegisteredRingVrfKeyScale(handle.toScale(), rings.map { it.toScale() }, publicKey)
