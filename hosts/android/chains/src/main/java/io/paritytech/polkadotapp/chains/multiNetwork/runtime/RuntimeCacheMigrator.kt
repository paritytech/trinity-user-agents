package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import javax.inject.Inject
import javax.inject.Singleton

/**
 * Forces a one-off metadata re-fetch when the *local* representation of cached metadata changes,
 * even though the chain's spec version has not.
 *
 * Bump [LATEST_VERSION] whenever already-cached metadata must be discarded: existing rows carry the
 * previous version, so every chain re-syncs exactly once and then persists the new one.
 */
@Singleton
class RuntimeCacheMigrator @Inject constructor() {
    companion object {
        // 2: move every client onto metadata v16, required for view functions
        private const val LATEST_VERSION = 2
    }

    fun needsMetadataFetch(localVersion: Int): Boolean {
        return localVersion < LATEST_VERSION
    }

    fun latestVersion(): Int {
        return LATEST_VERSION
    }
}
