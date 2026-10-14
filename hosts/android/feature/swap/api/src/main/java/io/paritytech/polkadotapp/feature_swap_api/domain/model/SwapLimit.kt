package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.asFraction
import io.paritytech.polkadotapp.common.utils.atLeastZero
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import java.math.BigDecimal

sealed class SwapLimit {
    companion object;

    data class SpecifiedIn(
        val amountIn: Balance,
        val amountOutQuote: Balance,
        val amountOutMin: Balance
    ) : SwapLimit()

    data class SpecifiedOut(
        val amountOut: Balance,
        val amountInQuote: Balance,
        val amountInMax: Balance
    ) : SwapLimit()
}

val SwapLimit.swapDirection: SwapDirection
    get() = when (this) {
        is SwapLimit.SpecifiedIn -> SwapDirection.SPECIFIED_IN
        is SwapLimit.SpecifiedOut -> SwapDirection.SPECIFIED_OUT
    }

val SwapLimit.quotedAmount: Balance
    get() = when (this) {
        is SwapLimit.SpecifiedIn -> amountIn
        is SwapLimit.SpecifiedOut -> amountOut
    }

val SwapLimit.estimatedAmountIn: Balance
    get() = when (this) {
        is SwapLimit.SpecifiedIn -> amountIn
        is SwapLimit.SpecifiedOut -> amountInQuote
    }

val SwapLimit.amountOutMin: Balance
    get() = when (this) {
        is SwapLimit.SpecifiedIn -> amountOutMin
        is SwapLimit.SpecifiedOut -> amountOut
    }

val SwapLimit.estimatedAmountOut: Balance
    get() = when (this) {
        is SwapLimit.SpecifiedIn -> amountOutQuote
        is SwapLimit.SpecifiedOut -> amountOut
    }

/**
 * Adjusts SwapLimit to the [newAmountIn] based on the quoted swap rate
 * This is only suitable for small changes amount in, as it implicitly assumes the swap rate stays the same
 */
fun SwapLimit.replaceAmountIn(newAmountIn: Balance, shouldReplaceBuyWithSell: Boolean): SwapLimit {
    return when (this) {
        is SwapLimit.SpecifiedIn -> updateInAmount(newAmountIn)
        is SwapLimit.SpecifiedOut -> {
            if (shouldReplaceBuyWithSell) {
                updateInAmountChangingToSell(newAmountIn)
            } else {
                updateInAmount(newAmountIn)
            }
        }
    }
}

fun SwapLimit.Companion.createAggregated(firstLimit: SwapLimit, lastLimit: SwapLimit): SwapLimit {
    return when (firstLimit) {
        is SwapLimit.SpecifiedIn -> {
            require(lastLimit is SwapLimit.SpecifiedIn)

            SwapLimit.SpecifiedIn(
                amountIn = firstLimit.amountIn,
                amountOutQuote = lastLimit.amountOutQuote,
                amountOutMin = lastLimit.amountOutMin
            )
        }

        is SwapLimit.SpecifiedOut -> {
            require(lastLimit is SwapLimit.SpecifiedOut)

            SwapLimit.SpecifiedOut(
                amountOut = lastLimit.amountOut,
                amountInQuote = firstLimit.amountInQuote,
                amountInMax = firstLimit.amountInMax
            )
        }
    }
}

private fun SwapLimit.SpecifiedOut.updateInAmountChangingToSell(newAmountIn: Balance): SwapLimit {
    val slippage = slippage()

    val inferredQuotedBalance = replacedInQuoteAmount(newAmountIn, amountOut)

    return SpecifiedIn(amount = newAmountIn, slippage, quotedBalance = inferredQuotedBalance)
}

private fun SwapLimit.SpecifiedOut.slippage(): Fraction {
    if (amountInQuote.isZero()) return Fraction.ZERO

    val slippageAsFraction = (amountInMax / amountInQuote - BigDecimal.ONE).atLeastZero()
    return slippageAsFraction.asFraction
}

private fun SwapLimit.SpecifiedIn.replaceInMultiplier(amount: Balance): BigDecimal {
    return amount / amountIn
}

private fun SwapLimit.SpecifiedIn.replacingInAmount(newInAmount: Balance, replacingAmount: Balance): Balance {
    return newInAmount * replaceInMultiplier(replacingAmount)
}

private fun SwapLimit.SpecifiedIn.updateInAmount(newAmountIn: Balance): SwapLimit.SpecifiedIn {
    return SwapLimit.SpecifiedIn(
        amountIn = newAmountIn,
        amountOutQuote = replacingInAmount(newAmountIn, replacingAmount = amountOutQuote),
        amountOutMin = replacingInAmount(newAmountIn, replacingAmount = amountOutMin)
    )
}

private fun SwapLimit.SpecifiedOut.replaceInQuoteMultiplier(amount: Balance): BigDecimal {
    return amount / amountInQuote
}

private fun SwapLimit.SpecifiedOut.replacedInQuoteAmount(newInQuoteAmount: Balance, replacingAmount: Balance): Balance {
    return newInQuoteAmount * replaceInQuoteMultiplier(replacingAmount)
}

private fun SwapLimit.SpecifiedOut.updateInAmount(newAmountInQuote: Balance): SwapLimit.SpecifiedOut {
    return SwapLimit.SpecifiedOut(
        amountOut = replacedInQuoteAmount(newAmountInQuote, amountOut),
        amountInQuote = newAmountInQuote,
        amountInMax = replacedInQuoteAmount(newAmountInQuote, amountInMax)
    )
}

fun SwapQuote.toFeeArgs(
    slippage: Fraction,
    firstSegmentFees: Chain.Asset,
    sender: MetaAccount,
    recipient: AccountId,
): SwapFeeArgs {
    return SwapFeeArgs(
        assetIn = amountIn.chainAsset,
        slippage = slippage,
        direction = quotedPath.direction,
        executionPath = quotedPath.path.map { quotedSwapEdge -> SegmentExecuteArgs(quotedSwapEdge) },
        firstSegmentFees = firstSegmentFees,
        sender = sender,
        recipient = recipient
    )
}

fun SwapLimit(direction: SwapDirection, amount: Balance, slippage: Fraction, quotedBalance: Balance): SwapLimit {
    return when (direction) {
        SwapDirection.SPECIFIED_IN -> SpecifiedIn(amount, slippage, quotedBalance)
        SwapDirection.SPECIFIED_OUT -> SpecifiedOut(amount, slippage, quotedBalance)
    }
}

private fun SpecifiedIn(amount: Balance, slippage: Fraction, quotedBalance: Balance): SwapLimit.SpecifiedIn {
    val lessAmountCoefficient = BigDecimal.ONE - slippage.fraction
    val amountOutMin = quotedBalance * lessAmountCoefficient

    return SwapLimit.SpecifiedIn(
        amountIn = amount,
        amountOutQuote = quotedBalance,
        amountOutMin = amountOutMin
    )
}

private fun SpecifiedOut(amount: Balance, slippage: Fraction, quotedBalance: Balance): SwapLimit.SpecifiedOut {
    val moreAmountCoefficient = BigDecimal.ONE + slippage.fraction
    val amountInMax = quotedBalance * moreAmountCoefficient

    return SwapLimit.SpecifiedOut(
        amountOut = amount,
        amountInQuote = quotedBalance,
        amountInMax = amountInMax
    )
}
