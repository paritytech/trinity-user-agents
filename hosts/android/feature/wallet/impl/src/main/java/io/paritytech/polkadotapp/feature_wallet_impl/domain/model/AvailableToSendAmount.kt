package io.paritytech.polkadotapp.feature_wallet_impl.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks

/**
 * [gainingPrivacy] is held back by the privacy strategy. It is only reachable once the user confirms they
 * accept losing the privacy it has earned, and only where the strategy makes that offer at all.
 */
data class AvailableToSendAmount(
    val spendable: Balance,
    val gainingPrivacy: Balance,
    val canSpendGainingPrivacy: Boolean,
    val chainAsset: Chain.Asset
) {
    /** Everything a send could draw on, so the input is not capped below what the confirmation allows. */
    val reachable: Balance = if (canSpendGainingPrivacy) spendable + gainingPrivacy else spendable

    /** Null when there is nothing extra to offer, so the caller can leave the hint out entirely. */
    val offerable: Balance? = gainingPrivacy.takeIf { canSpendGainingPrivacy && !it.isZero() }
}

fun AvailableToSendAmount.spendablePlanks() = chainAsset.amountFromPlanks(spendable)

fun AvailableToSendAmount.reachablePlanks() = chainAsset.amountFromPlanks(reachable)
