package io.paritytech.polkadotapp.feature_dotns_api.domain

import kotlinx.coroutines.delay
import kotlin.time.Duration.Companion.seconds

/**
 * The active network's dotNS TLD, successfully read once per process from the people chain's
 * network suffix. Failed reads are not cached.
 *
 * A chain that reports no usable suffix leaves the value unsettled, exactly as a transport failure
 * does - there is no substitute for a TLD the network did not report.
 */
interface DotNsTldProvider {
    /**
     * The settled TLD, else the value persisted by a previous process, else `null`.
     * A `null` answer kicks a background refresh; callers must treat it as
     * "not a dotNS decision", never substitute a guess for it.
     */
    fun currentTldOrNull(): DotNsTld?

    /** Gets the settled TLD, allowing only one contract read at a time. Fails on transport errors. */
    suspend fun getTld(): Result<DotNsTld>
}

/**
 * Gets the settled TLD, retrying transport failures until one lands — never fails and never
 * guesses. For key-derivation paths, where a fallback guess would mint key material that belongs
 * to no network.
 */
suspend fun DotNsTldProvider.getTldRetrying(): DotNsTld {
    while (true) {
        getTld().getOrNull()?.let { return it }
        delay(TLD_RETRY_INTERVAL)
    }
}

private val TLD_RETRY_INTERVAL = 3.seconds
