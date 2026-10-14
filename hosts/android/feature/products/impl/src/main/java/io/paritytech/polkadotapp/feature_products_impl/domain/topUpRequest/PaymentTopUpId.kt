package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap

private const val SIZE_BYTES = 32

/**
 * The product's own name for a top-up, and the only thing that identifies it.
 *
 * Supplied by the product rather than handed back by the host: a product can be terminated between asking
 * for a top-up and recording what it was called, and an id it never saw is one it can never ask about again.
 *
 * Opaque to the host, so it is never parsed or shortened — only compared, and rendered as hex where a
 * string is needed.
 */
@JvmInline
value class PaymentTopUpId private constructor(val bytes: DataByteArray) {
    fun asHex(): String = bytes.value.toHexString()

    companion object {
        fun fromBytes(bytes: DataByteArray): Result<PaymentTopUpId> {
            if (bytes.value.size != SIZE_BYTES) {
                return Result.failure(
                    IllegalArgumentException("PaymentTopUpId must be $SIZE_BYTES bytes, got ${bytes.value.size}")
                )
            }

            return Result.success(PaymentTopUpId(bytes))
        }

        fun fromHex(hex: String): Result<PaymentTopUpId> =
            runCatching { hex.hexToDataByteArray() }.flatMap { fromBytes(it) }
    }
}
