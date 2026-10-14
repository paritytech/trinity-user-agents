package io.paritytech.polkadotapp.feature_transactions.api.data.extensions

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.FixedValueTransactionExtension
import io.paritytech.polkadotapp.chains.util.structOf
import java.math.BigInteger

class ChargeAssetTxPayment(
    val assetId: Any? = null,
    val tip: BigInteger = BigInteger.ZERO
) : FixedValueTransactionExtension(
    name = ID,
    implicit = null,
    explicit = assetTxPaymentPayload(assetId, tip)
) {
    companion object {
        val ID = "ChargeAssetTxPayment"

        private fun assetTxPaymentPayload(assetId: Any?, tip: BigInteger = BigInteger.ZERO): Any {
            return structOf(
                "tip" to tip,
                "assetId" to assetId
            )
        }

        fun ExtrinsicBuilder.chargeAssetTxPayment(
            assetId: Any? = null,
            tip: BigInteger = BigInteger.ZERO
        ): ExtrinsicBuilder {
            return setTransactionExtension(ChargeAssetTxPayment(assetId, tip))
        }
    }
}
