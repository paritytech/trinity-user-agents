package io.paritytech.polkadotapp.feature_dotns_api.domain

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import kotlinx.coroutines.flow.Flow
import java.io.File

interface DotNsResolver {
    /**
     * The content of [dotNsName] from the version the device serves. Content the device has not loaded yet is read
     * from chain and downloaded, retried per [retryPolicy]; from then on it is served without chain.
     */
    suspend fun resolveToLocalUri(
        dotNsName: String,
        retryPolicy: NetworkRetryPolicy = NetworkRetryPolicy.DEFAULT,
    ): Result<Uri>

    /**
     * The text record [key] of [dotNsName] from the version the device serves. A record not read before is read from
     * chain, retried per [retryPolicy], and kept, so later reads, after a restart too, need no chain.
     */
    suspend fun getMetadataEntry(
        dotNsName: String,
        key: String,
        retryPolicy: NetworkRetryPolicy = NetworkRetryPolicy.DEFAULT,
    ): Result<String?>

    /**
     * Reads what [dotNsNames] point to on chain now and downloads their new content, without touching what is served.
     * The update covers all of them or none: if a read or a download fails, nothing is kept. True when any of them has
     * an update ready for [applyUpdates]. Content the device has not loaded is not read: its first load gets the latest.
     */
    suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Boolean>

    /** Serves the downloaded updates of [dotNsNames] together from the next request on and deletes what they replace. */
    suspend fun applyUpdates(dotNsNames: Collection<String>)

    /** Names with a downloaded update awaiting [applyUpdates]. */
    fun observePendingUpdates(): Flow<Set<String>>

    /** Names with content or records on the device. */
    fun namesOnDevice(): Set<String>

    /** Observes resolution progress for [dotNsName]. Emits [DotNsLoadProgress.Idle] when nothing is in flight. */
    fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress>

    suspend fun clearCache()
}

suspend fun DotNsResolver.resolveToLocalFile(
    dotNsName: String,
    retryPolicy: NetworkRetryPolicy = NetworkRetryPolicy.DEFAULT,
): Result<File> {
    return resolveToLocalUri(dotNsName, retryPolicy).map { File(it.path!!) }
}
