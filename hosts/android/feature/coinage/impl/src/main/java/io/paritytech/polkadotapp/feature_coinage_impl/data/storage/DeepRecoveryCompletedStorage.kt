package io.paritytech.polkadotapp.feature_coinage_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import javax.inject.Inject

// Whether the user closed the "balance restored" prompt, accepting what recovery found so far.
interface DeepRecoveryCompletedStorage : SingleValueStorage<Boolean>

class RealDeepRecoveryCompletedStorage @Inject constructor(
    factory: SingleValueStorageFactory,
) : DeepRecoveryCompletedStorage,
    SingleValueStorage<Boolean> by factory.preferences(
        key = "CoinsDeepBackupCompletedStorage",
        serializer = PreferencesSingleValueSerializer.from(
            toString = { it.toString() },
            fromString = { it.toBoolean() },
        ),
        default = false,
    )
