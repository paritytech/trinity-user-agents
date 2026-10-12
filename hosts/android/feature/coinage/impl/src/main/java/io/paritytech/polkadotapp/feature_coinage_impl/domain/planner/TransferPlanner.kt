package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAmountBreakdown
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ageOrDefault
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.isAgeValidToSpend
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.isInRecycler
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.recyclerLocationOrThrow
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions.InsufficientBalanceException
import java.math.BigDecimal

class TransferPlanner(
    private val conversionContext: CoinageBalanceConversionContext,
    private val breakdownAmount: CoinAmountBreakdown,
    private val coinMaxRecyclingAge: Int
) {
    fun plan(
        amount: BigDecimal,
        coins: List<Coin>,
        vouchers: List<RecyclerVoucher>
    ): Result<TransferPlan> {
        return runCatching {
            require(breakdownAmount.remainderAfterBreakdown(amount).isZero()) { "Amount $amount cannot be broken down into coins" }

            val strategyType = tryGetExactMatchPlan(amount, coins)
                ?: tryGetSingleSplitPlan(amount, coins)
                ?: tryGetCoinsAndUnloadPlan(amount, coins, vouchers)
                ?: throw InsufficientBalanceException()

            TransferPlan(strategyType)
        }
    }

    private fun tryGetExactMatchPlan(
        amount: BigDecimal,
        coins: List<Coin>
    ): StrategyType? {
        val selectedCoins = findSubsetSum(coins, amount) ?: return null

        return StrategyType.ExactCoins(coins = selectedCoins)
    }

    private fun tryGetSingleSplitPlan(
        amount: BigDecimal,
        coins: List<Coin>
    ): StrategyType? {
        val (selectedCoins, coveredAmount) = findMaxCoinCoverage(coins, amount)
        val notSelectedCoins = coins.filter { it !in selectedCoins }

        val restAmount = amount - coveredAmount

        if (restAmount <= BigDecimal.ZERO) throw IllegalStateException("Not needed to split coins: transfer amount may be covered by exact coins.")

        val coinToSplit = notSelectedCoins
            .filter { it.isAgeValidToSpend(coinMaxRecyclingAge) && it.valueExponent.toAmount() > restAmount }
            .minByOrNull { it.valueExponent }
            ?: return null

        val recipientDenominations = breakdownAmount.breakdown(restAmount)
        val changeDenominations = breakdownAmount.breakdown(coinToSplit.valueExponent.toAmount() - restAmount)

        return StrategyType.Split(
            splitFrom = coinToSplit,
            recipientDenominations = recipientDenominations,
            changeDenominations = changeDenominations,
            exactCoins = selectedCoins
        )
    }

    private fun tryGetCoinsAndUnloadPlan(
        amount: BigDecimal,
        coins: List<Coin>,
        vouchers: List<RecyclerVoucher>
    ): StrategyType? {
        val (selectedCoins, coveredCoinAmount) = findMaxCoinCoverage(coins, amount)

        val remainingAmount = amount - coveredCoinAmount

        if (remainingAmount <= BigDecimal.ZERO) throw IllegalStateException("Not needed to unload vouchers: transfer amount may be covered by exact coins.")

        val readyVouchers = vouchers.filter { it.isInRecycler() }

        val (selectedVouchers, _) = findMinimalVoucherCover(readyVouchers, remainingAmount) ?: return null

        return StrategyType.UnloadAndSplit(
            vouchersToUnload = selectedVouchers,
            recipientAmount = remainingAmount,
            exactCoins = selectedCoins
        )
    }

    private fun findSubsetSum(coins: List<Coin>, target: BigDecimal): List<Coin>? {
        if (target.compareTo(BigDecimal.ZERO) == 0) return emptyList()
        if (target < BigDecimal.ZERO) return null

        val sortedCoins = coins
            .filter { it.isAgeValidToSpend(coinMaxRecyclingAge) }
            .sortedWith(
                compareByDescending<Coin> { it.valueExponent }.thenByDescending { it.ageOrDefault() }
            )

        var remaining = target
        val result = mutableListOf<Coin>()

        for (coin in sortedCoins) {
            if (remaining <= BigDecimal.ZERO) break
            val value = coin.valueExponent.toAmount()
            if (value <= remaining) {
                result.add(coin)
                remaining -= value
            }
        }

        return if (remaining.compareTo(BigDecimal.ZERO) == 0) result else null
    }

    private fun findMaxCoinCoverage(coins: List<Coin>, targetAmount: BigDecimal): Pair<List<Coin>, BigDecimal> {
        // Must apply the same spendability filter as findSubsetSum (exact match). Otherwise a
        // A past-recycling-age coin is excluded from exact match
        // gets included here, can cover the amount exactly, and trips the "restAmount == 0"
        // invariant in tryGetSingleSplitPlan/tryGetCoinsAndUnloadPlan.
        val sortedCoins = coins
            .filter { it.isAgeValidToSpend(coinMaxRecyclingAge) }
            .sortedWith(
                compareByDescending<Coin> { it.valueExponent }.thenByDescending { it.ageOrDefault() }
            )
        val selected = mutableListOf<Coin>()
        var covered = BigDecimal.ZERO

        for (coin in sortedCoins) {
            val newAmount = covered + coin.valueExponent.toAmount()
            if (newAmount <= targetAmount) {
                selected.add(coin)
                covered = newAmount
            } else {
                break
            }
        }

        return selected to covered
    }

    private fun findMinimalVoucherCover(
        vouchers: List<RecyclerVoucher>,
        targetAmount: BigDecimal
    ): Pair<List<RecyclerVoucher>, BigDecimal>? {
        val orderedVouchers = vouchers.orderForUnload()

        val selectedVouchers = mutableListOf<RecyclerVoucher>()
        var totalAmount = BigDecimal.ZERO
        var remaining = targetAmount

        for (voucher in orderedVouchers) {
            if (remaining <= BigDecimal.ZERO) break
            val amount = voucher.recyclerValue.toAmount()
            selectedVouchers += voucher
            totalAmount += amount
            remaining -= amount
        }

        return if (remaining <= BigDecimal.ZERO) selectedVouchers to totalAmount else null
    }

    private fun List<RecyclerVoucher>.orderForUnload(): List<RecyclerVoucher> {
        return groupBy {
            VoucherUnloadGroupKey(
                valueExponent = it.recyclerValue,
                recyclerIndex = it.recyclerLocationOrThrow().recyclerIndex
            )
        }.values
            .map { group -> group to group.sumOf { it.recyclerValue.toAmount() } }
            .sortedByDescending { (_, total) -> total }
            .flatMap { (group, _) -> group }
    }

    private data class VoucherUnloadGroupKey(
        val valueExponent: ValueExponent,
        val recyclerIndex: RecyclerIndex
    )

    private fun ValueExponent.toAmount() = conversionContext.formatExponentToAmount(this)
}
