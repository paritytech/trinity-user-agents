package io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap

private const val SIZE_BYTES = 32

/**
 * The product's own name for a payment request, and the only thing that identifies it.
 *
 * Supplied by the product rather than handed back by the host, for the same reason as a top-up id: a product
 * terminated between asking and recording the answer could otherwise never ask about the payment again.
 */
@JvmInline
value class ProductPaymentRequestId private constructor(val bytes: DataByteArray) {
    fun asHex(): String = bytes.value.toHexString()

    companion object {
        fun fromBytes(bytes: DataByteArray): Result<ProductPaymentRequestId> {
            if (bytes.value.size != SIZE_BYTES) {
                return Result.failure(
                    IllegalArgumentException("ProductPaymentRequestId must be $SIZE_BYTES bytes, got ${bytes.value.size}")
                )
            }

            return Result.success(ProductPaymentRequestId(bytes))
        }

        fun fromHex(hex: String): Result<ProductPaymentRequestId> =
            runCatching { hex.hexToDataByteArray() }.flatMap { fromBytes(it) }
    }
}
