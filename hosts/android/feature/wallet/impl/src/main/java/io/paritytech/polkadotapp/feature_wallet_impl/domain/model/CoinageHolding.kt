package io.paritytech.polkadotapp.feature_wallet_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent

/**
 * One holding, reduced to what the depiction needs: which denomination it is, which of the two buckets it
 * falls in, and everything that decides how worn its coin is drawn.
 *
 * Coins and vouchers are one shape here on purpose. The card draws a holding as a coin whatever it is
 * technically made of, and the difference between the two only ever mattered to the recycler.
 */
data class CoinageHolding(
    /** Stable across refreshes, so a coin keeps its identity — and its face — while the field re-lays out. */
    val id: String,
    val exponent: ValueExponent,
    val derivationIndex: CoinageKeyIndex,
    /** Spendable now at no privacy cost. Everything else is Clearing, whatever the reason. */
    val isReady: Boolean,
    /** Null when the holding's recycler was never observed, which is every coin received from a peer. */
    val recyclerFungibility: RecyclerFungibility?,
    /**
     * Left a recycler as one of a batch, which links it to everything that left with it — something the
     * recycler's own score knows nothing about.
     */
    val isBatchUnloaded: Boolean,
    /** Payments this holding has been through. Each one leaves a pit on the coin's face. */
    val hops: Int
)

/** Rows from two installations can share an item number, so both halves of the key are read. */
val holdingKeyOrder: Comparator<CoinageHolding> =
    DataByteArray.compareByBytes<CoinageHolding>(unsigned = true) { it.derivationIndex.installation.value.value }
        .thenBy { it.derivationIndex.item }
