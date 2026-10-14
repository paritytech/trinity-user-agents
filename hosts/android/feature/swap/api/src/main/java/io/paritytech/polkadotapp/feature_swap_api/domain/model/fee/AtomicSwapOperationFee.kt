package io.paritytech.polkadotapp.feature_swap_api.domain.model.fee

import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee

interface AtomicSwapOperationFee {
    /**
     * Fee that is paid when submitting transaction
     */
    val submissionFee: AccountFee

    val postSubmissionFees: PostSubmissionFees

    class PostSubmissionFees(
        /**
         * Post-submission fees paid by (some) origin account.
         * This is typed as `SubmissionFee` as those fee might still use different accounts (e.g. delivery fees are always paid from requested account)
         */
        val paidByAccount: List<AccountFee> = emptyList(),
        /**
         * Post-submission fees paid from swapping amount directly. Its payment is isolated and does not involve any withdrawals from accounts
         */
        val paidFromAmount: List<Fee> = emptyList()
    )
}
