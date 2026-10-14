package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.totalBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentPlanner
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.VoucherOffboarding
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinageAssetSelector
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.SpendScope
import javax.inject.Inject

class RealExternalPaymentPlanner @Inject constructor(
    private val assetSelector: CoinageAssetSelector,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase,
) : ExternalPaymentPlanner {
    override suspend fun plan(amount: Balance): Result<ExternalPaymentPlan> = runCancellableCatching {
        val converter = coinageBalanceConverterUseCase.create().getOrThrow()
        with(converter) { determinePlan(amount) }
    }

    override suspend fun canPayPrivately(amount: Balance): Result<Boolean> =
        coinageBalanceConverterUseCase.create().flatMap { converter ->
            runCancellableCatching { with(converter) { privateVouchers().totalBalance() >= amount } }
        }

    override suspend fun pickOffboarding(
        availableVouchers: List<RecyclerVoucher>,
        target: Balance,
    ): Result<VoucherOffboarding> = runCancellableCatching {
        val converter = coinageBalanceConverterUseCase.create().getOrThrow()
        with(converter) { pickVoucherForOffboardingOrThrow(availableVouchers, target, preferred = emptyList()) }
    }

    /**
     * Vouchers before coins, even ones still gaining privacy: a coin loaded only to be unloaded right away leaves
     * its recycler as soon as such a voucher would, so it saves no privacy and only adds a recycling round.
     */
    context(coinageContext: CoinageBalanceConversionContext)
    private suspend fun determinePlan(amount: Balance): ExternalPaymentPlan {
        val privateVouchers = privateVouchers()

        if (privateVouchers.totalBalance() >= amount) {
            return ExternalPaymentPlan.Ready(pickVoucherForOffboardingOrThrow(privateVouchers, amount, preferred = emptyList()))
        }

        val vouchers = assetSelector.getOnChainSpendableVouchers()

        if (vouchers.totalBalance() >= amount) {
            return ExternalPaymentPlan.Ready(pickVoucherForOffboardingOrThrow(vouchers, amount, preferred = privateVouchers))
        }

        val deficit = amount - vouchers.totalBalance()
        val coins = assetSelector.getRecyclableCoins()

        if (coins.totalBalance() >= deficit) {
            return ExternalPaymentPlan.LoadCoins(
                coinsToLoad = pickCoinsForDeficit(coins, deficit),
                exactVouchers = vouchers,
            )
        }

        return ExternalPaymentPlan.NotEnoughAmount(
            activeVouchers = vouchers.totalBalance(),
            activeCoins = coins.totalBalance(),
            deficitToCoverWithCoins = deficit,
        )
    }

    private suspend fun privateVouchers() = assetSelector.getSelectableVouchers(SpendScope.SPENDABLE)

    /** Takes [preferred] vouchers first, then the largest, until [target] is reached. */
    context(coinageContext: CoinageBalanceConversionContext)
    private fun pickVoucherForOffboardingOrThrow(
        vouchers: List<RecyclerVoucher>,
        target: Balance,
        preferred: List<RecyclerVoucher>,
    ): VoucherOffboarding {
        require(vouchers.totalBalance() >= target) {
            "Insufficient vouchers balance ${vouchers.totalBalance()} to cover target $target"
        }

        val preferredKeys = preferred.mapToSet { it.ringVrfKeyIndex }
        val sorted = vouchers.sortedWith(
            compareByDescending<RecyclerVoucher> { it.ringVrfKeyIndex in preferredKeys }
                .thenByDescending { coinageContext.formatExponentToBalance(it.recyclerValue) }
        )

        val selected = mutableListOf<RecyclerVoucher>()
        var accumulated = Balance.ZERO

        for (voucher in sorted) {
            if (accumulated >= target) break
            selected.add(voucher)
            accumulated += coinageContext.formatExponentToBalance(voucher.recyclerValue)
        }

        val surplus = (accumulated - target)

        return VoucherOffboarding(selected, surplus)
    }

    context(coinageContext: CoinageBalanceConversionContext)
    private fun pickCoinsForDeficit(
        coins: List<Coin>,
        deficitPlanks: Balance,
    ): List<Coin> {
        val sorted = coins.sortedByDescending { coinageContext.formatExponentToBalance(it.valueExponent) }

        val selected = mutableListOf<Coin>()
        var accumulated = Balance.ZERO

        for (coin in sorted) {
            if (accumulated >= deficitPlanks) break
            selected.add(coin)
            accumulated += coinageContext.formatExponentToBalance(coin.valueExponent)
        }

        return selected
    }
}
