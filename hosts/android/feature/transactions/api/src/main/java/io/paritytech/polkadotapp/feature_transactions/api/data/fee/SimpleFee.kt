package io.paritytech.polkadotapp.feature_transactions.api.data.fee

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee

class SimpleFee(
    override val amount: Balance,
    override val asset: Chain.Asset,
) : Fee {
    override fun toString(): String {
        return "${asset.amountFromPlanks(amount)} ${asset.symbol}"
    }
}

class SimpleAccountFee(
    override val origin: AccountId,
    override val amount: Balance,
    override val asset: Chain.Asset
) : AccountFee {
    override fun toString(): String {
        return "${asset.amountFromPlanks(amount)} ${asset.symbol} by ${origin.value.toHexString()}"
    }
}
