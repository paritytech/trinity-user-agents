package io.paritytech.polkadotapp.common.data.cache

/**
 * A hint that can be passed from domain layer to repositories
 * to specify requirements for data consistency with remote source
 */
enum class CacheableDataConsistency {
    /**
     * Data must be up-to-date with the remote source.
     * The repository should fetch fresh data from remote, bypassing or invalidating the cache.
     */
    CONSISTENT_WITH_REMOTE,

    /**
     * Data is allowed to be stale. The repository may return cached data without consulting the remote source.
     */
    CAN_BE_STALE
}
