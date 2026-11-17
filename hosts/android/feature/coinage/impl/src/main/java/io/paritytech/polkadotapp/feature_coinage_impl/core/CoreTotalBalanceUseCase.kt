package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.Balance
import uniffi.truapi.NativeBalanceObserver
import uniffi.truapi.PrivacyPreset
import javax.inject.Inject

class CoreTotalBalanceUseCase @Inject constructor(
    private val core: CoinageCore,
    private val strategySettings: CoinageRecyclingStrategySettings,
) : TotalBalanceUseCase {
    override fun subscribeTotalBalance(): Flow<Result<CoinageBalance>> = flow {
        val units = core.units().getOrElse { throw it }
        val balances = core.observe<Balance> { coinage, push ->
            coinage.observeBalance(
                object : NativeBalanceObserver {
                    override fun onBalance(balance: Balance) = push(balance)
                }
            )
        }
        balances.combine(strategySettings.strategyFlow()) { balance, strategy -> balance.toCoinageBalance(units, strategy) }
            .map { Result.success(it) }
            .collect { emit(it) }
    }.catch { emit(Result.failure(it)) }

    override suspend fun getBalance(): Result<CoinageBalance> = core.units().flatMap { units ->
        runCancellableCatching { strategySettings.getStrategy() }.flatMap { strategy ->
            core.use { coinage -> coinage.balance().toCoinageBalance(units, strategy) }
        }
    }
}

fun Balance.toCoinageBalance(units: CoinageUnits, strategy: RecyclingStrategyType) = CoinageBalance(
    availablePrivate = units.balanceOf(spendable),
    gainingPrivacy = CoinageBalance.GainingPrivacyBalance(
        amount = units.balanceOf(heldBack),
        canSpendWithConfirmation = strategy.allowsConfirmedSpend(),
    ),
    pending = units.balanceOf(arriving),
)

/** Whether the user may spend value that is still gaining privacy, once they confirm it. */
fun RecyclingStrategyType.allowsConfirmedSpend(): Boolean = this != RecyclingStrategyType.MAX_PRIVACY

/** The core's preset for the strategy the user chose. */
fun RecyclingStrategyType.toPreset(): PrivacyPreset = when (this) {
    RecyclingStrategyType.MIN_PRIVACY -> PrivacyPreset.FASTEST
    RecyclingStrategyType.BALANCED -> PrivacyPreset.BALANCED
    RecyclingStrategyType.MAX_PRIVACY -> PrivacyPreset.MOST_PRIVATE
}
