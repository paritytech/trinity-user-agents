package io.paritytech.polkadotapp.feature_usernames_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

private const val QUEUED_CLAIM_KEY = "QueuedClaimUsername"

class QueuedClaimStorage(
    delegate: SingleValueStorage<Username>
) : SingleValueStorage<Username> by delegate

fun SingleValueStorageFactory.queuedClaimStorage(): QueuedClaimStorage {
    val storage = preferences(
        key = QUEUED_CLAIM_KEY,
        serializer = PreferencesSingleValueSerializer.from(
            toString = Username::getDisplayUsername,
            fromString = Username::fromFullValue
        ),
        default = null
    )
    return QueuedClaimStorage(storage)
}
