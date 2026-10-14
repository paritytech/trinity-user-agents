package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.NativeFeePayment
import java.math.BigDecimal

data class CrossChainTransfer(
    val direction: CrossChainTransferDirection,
    val userSpecifiedAmount: Balance,
    val recipient: AccountId,
    val amountAdjustmentMode: AmountAdjustmentMode,
    val originFeePayment: FeePayment = NativeFeePayment(),
)

val CrossChainTransfer.destinationChainAsset: Chain.Asset
    get() = direction.to.asset

val CrossChainTransfer.destinationChain: Chain
    get() = direction.to.chain

val CrossChainTransfer.originChainAsset: Chain.Asset
    get() = direction.from.asset

val CrossChainTransfer.originChain: Chain
    get() = direction.from.chain

val CrossChainTransfer.withdrawAmount: Balance
    get() = when (amountAdjustmentMode) {
        AmountAdjustmentMode.Exact -> userSpecifiedAmount
        is AmountAdjustmentMode.OffsetFees -> userSpecifiedAmount + amountAdjustmentMode.feesPaidFromAmount
    }

fun CrossChainTransfer.decimalWithdrawAmount(): BigDecimal {
    return originChainAsset.amountFromPlanks(withdrawAmount)
}
