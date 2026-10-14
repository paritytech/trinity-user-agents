package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAmountBreakdown
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.recyclerLocationOrThrow
import java.math.BigDecimal

data class VoucherBatch(
    val recyclerKey: RecyclerKey,
    val vouchers: List<RecyclerVoucher>,
    val recipientDenominations: List<ValueExponent>,
    val changeDenominations: List<ValueExponent>
)

/**
 * Decides how vouchers are cashed in for a transfer.
 *
 * Vouchers are unloaded in batches; each batch redeems vouchers of one size (`2^exponent`), so a batch can only
 * produce coins whose total is at most `voucherCount * 2^exponent`, and every coin must come whole out of one batch.
 *
 * The problem this avoids: breaking the *whole* recipient amount into coins first can yield a coin larger than any
 * single batch (e.g. a $10.24 coin when the biggest batch is one $5.12 voucher), which is impossible to unload even
 * though the balance is sufficient. Instead we decide each batch's value first, then break that value into coins — so
 * every coin is, by construction, no larger than the batch it comes from and always fits.
 *
 * Recipient value is filled batch by batch; whatever a batch has left over after the recipient is covered becomes
 * change. The total of all batch values equals recipient + change, so the recipient is always fully covered.
 */
object VoucherBatchDistribution {
    fun distribute(
        vouchers: List<RecyclerVoucher>,
        recipientAmount: BigDecimal,
        maxConsolidation: Int,
        breakdown: CoinAmountBreakdown,
        conversionContext: CoinageBalanceConversionContext
    ): List<VoucherBatch> {
        val batches = vouchers
            .groupBy { RecyclerKey(it.recyclerValue, it.recyclerLocationOrThrow().recyclerIndex) }
            .flatMap { (key, group) -> group.chunked(maxConsolidation).map { chunk -> key to chunk } }

        var recipientRemaining = recipientAmount

        return batches.map { (key, batchVouchers) ->
            val batchValue = conversionContext.formatExponentToAmount(key.exponent) * batchVouchers.size.toBigDecimal()

            val recipientPart = batchValue.min(recipientRemaining)
            recipientRemaining -= recipientPart
            val changePart = batchValue - recipientPart

            VoucherBatch(
                recyclerKey = key,
                vouchers = batchVouchers,
                recipientDenominations = breakdown.breakdown(recipientPart),
                changeDenominations = breakdown.breakdown(changePart)
            )
        }
    }
}
