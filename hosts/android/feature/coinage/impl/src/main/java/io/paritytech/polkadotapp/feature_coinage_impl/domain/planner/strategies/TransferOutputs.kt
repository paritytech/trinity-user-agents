package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin

/** Coins minted as the output of a transfer: those handed to the recipient and the change kept by the sender. */
data class TransferOutputs(
    val recipient: List<Coin>,
    val change: List<Coin>
) {
    val all: List<Coin> get() = recipient + change
}
