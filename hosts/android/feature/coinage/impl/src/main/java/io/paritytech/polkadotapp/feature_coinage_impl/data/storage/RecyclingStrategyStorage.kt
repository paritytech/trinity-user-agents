package io.paritytech.polkadotapp.feature_coinage_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType

internal typealias RecyclingStrategyStorage = SingleValueStorage<RecyclingStrategyType>

private const val KEY_RECYCLING_STRATEGY = "coinage_recycling_strategy"

internal fun SingleValueStorageFactory.createRecyclingStrategyStorage(): RecyclingStrategyStorage {
    return preferences(
        key = KEY_RECYCLING_STRATEGY,
        default = RecyclingStrategyType.BALANCED,
        serializer = PreferencesSingleValueSerializer.from(
            toString = { it.name },
            fromString = { name ->
                RecyclingStrategyType.entries.firstOrNull { it.name == name } ?: RecyclingStrategyType.BALANCED
            }
        )
    )
}
