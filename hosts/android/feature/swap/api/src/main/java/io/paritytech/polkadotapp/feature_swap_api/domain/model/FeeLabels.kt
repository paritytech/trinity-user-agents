package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee

class AccountFeeWithLabel(
    val fee: AccountFee,
    val debugLabel: String = "Submission"
) : AccountFee by fee {
    override fun toString(): String {
        return "$fee ($debugLabel)"
    }
}

fun AccountFeeWithLabel(fee: AccountFee?, debugLabel: String): AccountFeeWithLabel? {
    return fee?.let { AccountFeeWithLabel(it, debugLabel) }
}

class FeeWithLabel(
    val fee: Fee,
    val debugLabel: String
) : Fee by fee {
    override fun toString(): String {
        return "$fee ($debugLabel)"
    }
}
