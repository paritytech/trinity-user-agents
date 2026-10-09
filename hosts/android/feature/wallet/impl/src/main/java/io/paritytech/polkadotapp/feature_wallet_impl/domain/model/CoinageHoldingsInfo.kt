package io.paritytech.polkadotapp.feature_wallet_impl.domain.model

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinRecyclingState
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.hops
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.recyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.coinageBalanceOf
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldings

/** What the coinage card renders: the partition of the total, and every item making it up. */
data class CoinageHoldingsInfo(
    val balance: CoinageBalance,
    val holdings: List<CoinageHolding>
)

/** Both halves of what the card shows, from one classification — see [CoinageHoldingsInfo]. */
context(conversion: CoinageBalanceConversionContext)
fun CoinageHoldings.toHoldingsInfo() = CoinageHoldingsInfo(
    balance = coinageBalanceOf(coins, verdicts, vouchers, canSpendWithConfirmation),
    holdings = toRows()
)

/**
 * The domain classifies a holding three ways; the user is shown two.
 *
 * Ready is what the balance calls `availablePrivate` — minted coins the strategy allows, and vouchers whose
 * ring already satisfies it. Clearing is everything else, whatever the reason: earning privacy, forced to
 * recycle, or still arriving. Telling the user which would put a third state on screen that was deliberately
 * removed, and the same partition has to drive the figures, the ordering and the coins or they disagree.
 *
 * Deliberately not [io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts]'
 * spend test: that one releases the gaining-privacy bucket when the strategy sells privacy back, which is a
 * question about what the user *may* do rather than about what is ready.
 */
private fun CoinageHoldings.toRows(): List<CoinageHolding> {
    val ready = coins.minted
        .filter { verdicts[it.derivationIndex] == CoinRecyclingState.ALLOW_USE }
        .mapTo(HashSet()) { it.derivationIndex }
    val readyVouchers = vouchers.usable.mapTo(HashSet()) { it.ringVrfKeyIndex }

    val coinRows = coins.total.map { it.toRow(isReady = it.derivationIndex in ready) }
    val voucherRows = vouchers.total.map { it.toRow(isReady = it.ringVrfKeyIndex in readyVouchers) }

    return coinRows + voucherRows
}

private fun Coin.toRow(isReady: Boolean) = CoinageHolding(
    id = "coin-${derivationIndex.installation.value}-${derivationIndex.item}",
    exponent = valueExponent,
    derivationIndex = derivationIndex,
    isReady = isReady,
    recyclerFungibility = recyclerFungibility,
    // A batch unload is identifiable without being recorded: the chain only hands out age 1 on one, and a
    // single unload leaves age 0.
    isBatchUnloaded = hops.isEmpty() && age == Coin.Age.Known(BATCH_UNLOAD_AGE),
    hops = hops.size
)

/** A voucher sitting in a recycler has been through no payments, so its coin is unpitted. */
private fun RecyclerVoucher.toRow(isReady: Boolean) = CoinageHolding(
    id = "voucher-${ringVrfKeyIndex.installation.value}-${ringVrfKeyIndex.item}",
    exponent = recyclerValue,
    derivationIndex = ringVrfKeyIndex,
    isReady = isReady,
    recyclerFungibility = recyclerFungibility,
    isBatchUnloaded = false,
    hops = 0
)

private const val BATCH_UNLOAD_AGE = 1
