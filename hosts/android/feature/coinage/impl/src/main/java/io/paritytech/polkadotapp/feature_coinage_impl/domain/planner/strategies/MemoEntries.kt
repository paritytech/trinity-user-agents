package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.PlannedMemoEntry

internal fun List<Coin>.toMemoEntries(): List<PlannedMemoEntry> = map {
    PlannedMemoEntry(coinDerivationIndex = it.derivationIndex, valueExponent = it.valueExponent)
}
