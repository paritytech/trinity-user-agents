package io.paritytech.polkadotapp.feature_device_sync_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Persisted monotonic counter for outgoing `SyncUpdateScale.id`s. Survives process restart so update
 * ids stay strictly increasing across sessions — peers reject duplicates and out-of-order ids.
 * Stored as Long for headroom, returned as UInt to match the wire format. Starts from 1.
 */
@Singleton
class SyncUpdateIdProvider @Inject constructor(
    private val preferences: Preferences,
) {
    private val mutex = Mutex()

    suspend fun nextId(): UInt = mutex.withLock {
        val next = preferences.getLong(KEY_NEXT_ID, 0L) + 1
        preferences.putLong(KEY_NEXT_ID, next)
        next.toUInt()
    }

    private companion object {
        const val KEY_NEXT_ID = "device_sync_next_update_id"
    }
}
