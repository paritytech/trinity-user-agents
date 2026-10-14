package io.paritytech.polkadotapp.feature_products_api.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import kotlinx.serialization.Serializable

@Serializable
sealed class ProductDerivationIndexScale {
    @Serializable
    @EnumIndex(0)
    class Plain(val index: UInt) : ProductDerivationIndexScale()

    // Bare ByteArray rather than DataByteArray: the binary scale encoder drops @FixedLength when
    // descending into a struct wrapper, which would emit a length prefix instead of the raw 32 bytes.
    @Serializable
    @EnumIndex(1)
    class Raw(
        @FixedLength(DerivationIndex32.SIZE_BYTES)
        val bytes: ByteArray,
    ) : ProductDerivationIndexScale()
}

/**
 * Re-narrows to [ProductDerivationIndexScale.Plain] whenever the selector carries the index magic:
 * plain is the primary form, and always emitting [ProductDerivationIndexScale.Raw] would put 32 bytes
 * on the wire where peers send a u32.
 */
fun DerivationIndex32.toScale(): ProductDerivationIndexScale {
    val plainIndex = asUIntOrNull()

    return if (plainIndex != null) {
        ProductDerivationIndexScale.Plain(plainIndex)
    } else {
        ProductDerivationIndexScale.Raw(bytes.value)
    }
}

fun ProductDerivationIndexScale.toDomain(): Result<DerivationIndex32> = when (this) {
    is ProductDerivationIndexScale.Plain -> Result.success(DerivationIndex32.fromUInt(index))
    is ProductDerivationIndexScale.Raw -> DerivationIndex32.fromBytes(bytes.toDataByteArray())
}
