package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_swap_api.domain.model.fee.AtomicSwapOperationFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.totalAmount
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.totalPlanksEnsuringAsset

interface AtomicSwapOperation {
    val estimatedSwapLimit: SwapLimit

    val assetIn: FullChainAssetId

    val assetOut: FullChainAssetId

    /**
     * Whether this exchange will take [AtomicSwapOperationSubmissionArgs.recipient] into account
     * False means that exchange only support conversions to the sender account and will do that
     * regardless of which recipient is specified
     */
    val supportsCustomRecipient: Boolean

    suspend fun estimateFee(): Result<AtomicSwapOperationFee>

    /**
     * Calculates how much of assetIn (of the current segment) is needed to buy given [extraOutAmount] of asset out (of the current segment)
     * Used to estimate how much extra amount of assetIn to add to the user input to accommodate future segment fees
     */
    suspend fun requiredAmountInToGetAmountOut(extraOutAmount: Balance): Balance

    /**
     * Additional amount that max amount calculation should leave aside for the **first** operation in the swap
     * One example is Existential Deposit in case operation executes in "keep alive" manner
     */
    suspend fun additionalMaxAmountDeduction(metaAccount: MetaAccount): SwapMaxAdditionalAmountDeduction

    /**
     * Executes itself with the given final args
     */
    suspend fun execute(args: AtomicSwapOperationSubmissionArgs): Result<SwapExecutionOutcome>
}

class AtomicSwapOperationSubmissionArgs(
    val actualSwapLimit: SwapLimit,
    val origin: TransactionOrigin,
    val recipient: AccountId
)

class AtomicSwapOperationArgs(
    val estimatedSwapLimit: SwapLimit,
    val feePayment: Chain.Asset,
)

fun AtomicSwapOperationFee.amountToLeaveOnOriginToPayTxFees(): Balance {
    val submissionAsset = submissionFee.asset
    return submissionFee.amount + postSubmissionFees.paidByAccount.totalAmount(submissionAsset, submissionFee.origin)
}

fun AtomicSwapOperationFee.totalFeeEnsuringSubmissionAsset(): Balance {
    val postSubmissionFeesByAccount = postSubmissionFees.paidByAccount.totalPlanksEnsuringAsset(submissionFee.asset)
    val postSubmissionFeesFromHolding = postSubmissionFees.paidByAccount.totalPlanksEnsuringAsset(submissionFee.asset)

    return submissionFee.amount + postSubmissionFeesByAccount + postSubmissionFeesFromHolding
}

/**
 * Collects all [Fee] instances from fee components
 */
fun AtomicSwapOperationFee.feeComponents(): List<Fee> {
    return buildList {
        add(submissionFee)
        postSubmissionFees.paidByAccount.onEach(::add)
        postSubmissionFees.paidFromAmount.onEach(::add)
    }
}

fun AtomicSwapOperationFee.allFeeAssets(): List<Chain.Asset> {
    return feeComponents()
        .map { it.asset }
        .distinctBy { it.fullId }
}

interface WithDepositedAmount {
    val actualReceivedAmount: Balance
}

class SwapExecutionOutcome(
    override val actualReceivedAmount: Balance,
) : WithDepositedAmount
