package io.paritytech.polkadotapp.feature_swap_impl.domain.fee

import io.paritytech.polkadotapp.feature_swap_api.domain.model.AccountFeeWithLabel
import io.paritytech.polkadotapp.feature_swap_api.domain.model.fee.AtomicSwapOperationFee
import io.paritytech.polkadotapp.feature_swap_api.domain.model.fee.AtomicSwapOperationFee.PostSubmissionFees
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee

class SubmissionOnlyAtomicSwapOperationFee(submissionFee: AccountFee) : AtomicSwapOperationFee {
    override val submissionFee: AccountFeeWithLabel = AccountFeeWithLabel(submissionFee)

    override val postSubmissionFees: PostSubmissionFees = PostSubmissionFees()
}
