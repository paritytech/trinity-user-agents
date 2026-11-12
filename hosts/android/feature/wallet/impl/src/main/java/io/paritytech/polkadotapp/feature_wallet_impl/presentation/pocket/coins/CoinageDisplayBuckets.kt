package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance

/**
 * The two buckets the card shows, as figures.
 *
 * The domain classifies a balance three ways; the user is only ever shown two. Clearing is everything that
 * is not spendable right now, whatever the reason: money in a recycler earning privacy, money the chain will
 * no longer accept until it is recycled, and money still arriving. The user does not need to know which, and
 * telling them would put a third state on screen that was deliberately removed.
 *
 * Together with [ready] this is exactly `total`, which is the invariant the two displayed figures have to
 * keep against the headline above them — and the same partition the coins are grouped into, so the run under
 * a label always counts the coins the label's figure is made of.
 */
val CoinageBalance.ready: Balance get() = availablePrivate

val CoinageBalance.clearing: Balance get() = gainingPrivacy.amount + pending
