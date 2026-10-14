package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee

data class CrossChainTransferFee(
    /**
     * Deducted upon initial transaction submission from the origin chain. Asset can be controlled with [FeePaymentCurrency]
     */
    val submissionFee: AccountFee,
    /**
     * Deducted upon initial transaction submission from the origin chain. Cannot be controlled with [FeePaymentCurrency]
     * and is always paid in native currency
     */
    val postSubmissionByAccount: AccountFee?,
    /**
     *  Total sum of all execution and delivery fees paid from holding register throughout xcm transfer
     *  Paid (at the moment) in a sending asset. There might be multiple [Chain.Asset] that represent the same logical asset,
     *  the asset here indicates the first one, on the origin chain
     */
    val postSubmissionFromAmount: Fee,
)
