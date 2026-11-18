package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHolding
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldings
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import uniffi.truapi.Asset
import uniffi.truapi.Holding
import uniffi.truapi.Holdings
import uniffi.truapi.NativeHoldingsObserver
import javax.inject.Inject

class CoreCoinageHoldingsUseCase @Inject constructor(
    private val core: CoinageCore,
    private val strategySettings: CoinageRecyclingStrategySettings,
) : CoinageHoldingsUseCase {
    override fun subscribeHoldings(): Flow<CoinageHoldings> = flow {
        val units = core.units().getOrElse { throw it }
        val holdings = core.observe<Holdings> { coinage, push ->
            coinage.observeHoldings(
                object : NativeHoldingsObserver {
                    override fun onHoldings(holdings: Holdings) = push(holdings)
                }
            )
        }
        emitAll(
            holdings.combine(strategySettings.strategyFlow()) { current, strategy ->
                CoinageHoldings(
                    balance = current.balance.toCoinageBalance(units, strategy),
                    holdings = current.rows.mapNotNull { it.toHolding(units) },
                )
            }
        )
    }
}

private fun Holding.toHolding(units: CoinageUnits): CoinageHolding? {
    val (kind, installation, index) = when (val asset = asset) {
        is Asset.Coin -> Triple("coin", asset.installation, asset.index)
        is Asset.Entry -> Triple("voucher", asset.installation, asset.index)
        // A peer's coin is not ours until a claim moves it into one of our own.
        is Asset.PeerCoin -> return null
    }
    val keyIndex = CoinageKeyIndex(CoinageInstallationId(installation.toDataByteArray()), index.toInt())
    return CoinageHolding(
        id = "$kind-${keyIndex.installation.value}-${keyIndex.item}",
        exponent = ValueExponent(exponent.toInt()),
        value = units.balanceOfExponent(exponent),
        keyIndex = keyIndex,
        isReady = ready,
        recyclerFungibility = fungibility?.let { RecyclerFungibility.ofPercent(it.toInt()) },
        isBatchUnloaded = batchUnloaded,
        // A batch unload starts a coin at age one rather than zero.
        hops = if (batchUnloaded) 0 else age?.toInt() ?: 0,
    )
}
