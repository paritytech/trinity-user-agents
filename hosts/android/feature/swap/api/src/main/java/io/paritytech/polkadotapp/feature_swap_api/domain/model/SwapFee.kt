package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_swap_api.domain.model.fee.AtomicSwapOperationFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.getAmount
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.totalAmount

class SwapFee(
    /**
     * Per swap segment fees
     */
    val segments: List<SwapSegment>,
    /**
     * Fee for the optional transfer that will deliver funds to the recipient
     */
    val recipientTransferFee: AccountFee?,
    /**
     *  Fees for second and subsequent segments converted to assetIn
     */
    private val intermediateSegmentFeesInAssetIn: Fee,
    /**
     * Additional deductions from max amount of asset in that are not directly caused by fees
     */
    private val additionalMaxAmountDeduction: SwapMaxAdditionalAmountDeduction,
    val sender: MetaAccount,
    val recipient: AccountId,
) {
    data class SwapSegment(val fee: AtomicSwapOperationFee, val operation: AtomicSwapOperation)

    val firstSegmentFee = segments.first().fee

    private val initialPostSubmissionFees = firstSegmentFee.postSubmissionFees

    private val assetIn = intermediateSegmentFeesInAssetIn.asset

    fun feeComponents(): List<Fee> {
        return segments.flatMap { it.fee.feeComponents() }
    }

    /**
     * Estimated amount that should be added on top of the swapping amount for recipient to receive
     * amount that is equal to the quote
     */
    fun additionalAmountForSwap(): Balance {
        val amountTakenFromAssetIn = initialPostSubmissionFees.paidFromAmount.totalAmount(assetIn)
        val totalFutureFeeInAssetIn = amountTakenFromAssetIn + intermediateSegmentFeesInAssetIn.amount

        return totalFutureFeeInAssetIn
    }

    /**
     * Balance that is needed for any swap that this fee represents, to succeed
     * This does not count the actual swap amount
     */
    fun requiredBalanceForMinimalSwap(): Balance {
        val initialSubmissionFee = firstSegmentFee.submissionFee.getAmount(assetIn)
        val additionalFeesAmount = initialPostSubmissionFees.paidByAccount.totalAmount(assetIn)
        val allExecutionFees = additionalAmountForSwap()
        val minimumRemaining = additionalMaxAmountDeduction.fromCountedTowardsEd

        return initialSubmissionFee + additionalFeesAmount + allExecutionFees + minimumRemaining
    }
}

fun SwapFee.swapNeedsToRecipientTransfer(): Boolean {
    return recipientTransferFee != null
}
