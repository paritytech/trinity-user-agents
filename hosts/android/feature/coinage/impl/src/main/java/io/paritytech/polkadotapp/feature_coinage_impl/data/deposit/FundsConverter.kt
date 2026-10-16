package io.paritytech.polkadotapp.feature_coinage_impl.data.deposit

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.model.AutoConversionCheckError
import io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.model.ConversionProgress
import io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.model.PossibleFundConversion
import io.paritytech.polkadotapp.feature_swap_api.domain.SwapService
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapDirection
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapFee
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapProgress
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapQuote
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapQuoteArgs
import io.paritytech.polkadotapp.feature_swap_api.domain.model.toFeeArgs
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

internal interface FundsConverter {
    context(scope: ComputationalScope)
    fun launchSync(): Flow<*>

    /**
     * error: [AutoConversionCheckError]
     */
    context(scope: ComputationalScope)
    suspend fun checkConversionPossible(
        tokenBalance: TokenBalance,
        destination: Chain.Asset,
        sender: MetaAccount,
        recipient: AccountId,
    ): Result<PossibleFundConversion>

    context(scope: ComputationalScope)
    suspend fun performConversion(
        checked: PossibleFundConversion,
    ): Flow<ConversionProgress>
}

internal class RealFundsConverter @Inject constructor(
    private val swapService: SwapService,
) : FundsConverter {
    companion object {
        private val SWAP_SLIPAGE = 0.5.percents
    }

    context(scope: ComputationalScope)
    override fun launchSync(): Flow<*> {
        return flow {
            swapService.sync()

            emitAll(swapService.runSubscriptions())
        }
    }

    context(scope: ComputationalScope)
    override suspend fun checkConversionPossible(
        tokenBalance: TokenBalance,
        destination: Chain.Asset,
        sender: MetaAccount,
        recipient: AccountId,
    ): Result<PossibleFundConversion> {
        swapService.awaitFullyLoadedRouting()

        return quoteTransferableConversion(tokenBalance, destination)
            .flatMap { preliminaryQuote -> calculateFee(preliminaryQuote, sender, recipient) }
            .flatMap { swapFee -> checkPassesDetectionThreshold(tokenBalance, swapFee) }
            .flatMap { swapFee -> determineMaxConvertibleBalance(tokenBalance, swapFee) }
            .flatMap { availableBalance -> quote(tokenBalance.token, destination, availableBalance) }
            .flatMap { actualQuote -> calculateFee(actualQuote, sender, recipient)
                .map { actualFee -> PossibleFundConversion(actualQuote, actualFee) }
            }
    }

    context(scope: ComputationalScope)
    override suspend fun performConversion(checked: PossibleFundConversion): Flow<ConversionProgress> {
        swapService.awaitFullyLoadedRouting()

        return swapService.swap(checked.fee)
            .map { it.toConversionProgress() }
    }

    private fun checkPassesDetectionThreshold(balance: TokenBalance, fee: SwapFee): Result<SwapFee> {
        val threshold = fee.detectableDepositThreshold()
        return if (balance.transferable >= threshold) {
            Result.success(fee)
        } else {
            val error = AutoConversionCheckError.NotEnoughBalance(balance.token, threshold, balance.transferable)
            Result.failure(error)
        }
    }

    private fun SwapFee.detectableDepositThreshold(): Balance {
        return minimumConversionDeposit() * 0.9
    }

    private fun SwapFee.minimumConversionDeposit(): Balance {
        // We're being conservative here since Cross chain transfer have a flaw where they put amount/2 into xcm execute which
        // effectively raises the bar for the minimum cross chain transfer
        return requiredBalanceForMinimalSwap() * 3
    }

    private fun SwapProgress.toConversionProgress(): ConversionProgress {
        return when (this) {
            is SwapProgress.Done -> ConversionProgress.Done(actualDeposited)
            is SwapProgress.SegmentStarted -> ConversionProgress.SwapInProgress(step)
            is SwapProgress.SegmentFailure -> ConversionProgress.Failed(error)
            SwapProgress.ToRecipientTransferStarted -> ConversionProgress.TransferToRecipient
            is SwapProgress.TransferFailure -> ConversionProgress.Failed(error)
        }
    }

    private fun determineMaxConvertibleBalance(
        balance: TokenBalance,
        fee: SwapFee
    ): Result<Balance> {
        val toLeaveAside = fee.requiredBalanceForMinimalSwap()
        val available = balance.transferable - toLeaveAside

        return if (available.isPositive()) {
            Result.success(available)
        } else {
            Result.failure(
                AutoConversionCheckError.NotEnoughBalance(
                    balance.token,
                    minRequired = toLeaveAside,
                    got = available
                )
            )
        }
    }

    context(scope: ComputationalScope)
    private suspend fun calculateFee(
        quote: SwapQuote,
        sender: MetaAccount,
        recipient: AccountId
    ): Result<SwapFee> {
        val feeArgs = quote.toFeeArgs(
            slippage = SWAP_SLIPAGE,
            firstSegmentFees = quote.assetIn,
            sender = sender,
            recipient = recipient
        )

        return swapService.estimateFee(feeArgs)
            .mapError(AutoConversionCheckError::FeeCalculationFailure)
    }

    context(scope: ComputationalScope)
    private suspend fun quoteTransferableConversion(
        tokenBalance: TokenBalance,
        destination: Chain.Asset,
    ): Result<SwapQuote> {
        return quote(tokenBalance.token, destination, tokenBalance.transferable)
    }

    context(scope: ComputationalScope)
    private suspend fun quote(
        from: Chain.Asset,
        to: Chain.Asset,
        amount: Balance
    ): Result<SwapQuote> {
        val quoteArgs = SwapQuoteArgs(from, to, amount, SwapDirection.SPECIFIED_IN)
        return swapService.quote(quoteArgs)
            .mapError(AutoConversionCheckError::QuoteFailure)
    }
}
