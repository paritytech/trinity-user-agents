package io.paritytech.polkadotapp.feature_account_impl.data.storage.newaccount

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.feature_account_api.data.storage.newaccount.NewAccountStorage
import javax.inject.Inject

class RealNewAccountStorage @Inject constructor(
    factory: SingleValueStorageFactory,
) : NewAccountStorage,
    SingleValueStorage<Boolean> by factory.preferences(
        key = "NewAccountStorage",
        serializer = PreferencesSingleValueSerializer.from(
            toString = { it.toString() },
            fromString = { it.toBoolean() },
        ),
        default = false,
    )
