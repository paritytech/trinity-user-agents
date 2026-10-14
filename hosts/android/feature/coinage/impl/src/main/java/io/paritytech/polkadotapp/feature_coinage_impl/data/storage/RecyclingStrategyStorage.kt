package io.paritytech.polkadotapp.feature_coinage_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType

internal typealias RecyclingStrategyStorage = SingleValueStorage<RecyclingStrategyType>

private const val KEY_RECYCLING_STRATEGY = "coinage_recycling_strategy"

/**
 * Defaults to the least private setting because it is what the app did before the choice existed — an
 * upgrade changes what the balance buckets are called, not which of them the user's money is in.
 */
internal fun SingleValueStorageFactory.createRecyclingStrategyStorage(): RecyclingStrategyStorage {
    return preferences(
        key = KEY_RECYCLING_STRATEGY,
        default = RecyclingStrategyType.MIN_PRIVACY,
        serializer = PreferencesSingleValueSerializer.from(
            toString = { it.name },
            fromString = { name ->
                RecyclingStrategyType.entries.firstOrNull { it.name == name } ?: RecyclingStrategyType.MIN_PRIVACY
            }
        )
    )
}
