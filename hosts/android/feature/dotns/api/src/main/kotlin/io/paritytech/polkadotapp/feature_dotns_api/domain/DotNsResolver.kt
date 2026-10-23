package io.paritytech.polkadotapp.feature_dotns_api.domain

import android.net.Uri
import kotlinx.coroutines.flow.Flow
import java.io.File

interface DotNsResolver {
    /**
     * The content of [dotNsName], served from the build on the device. A name with no build on the device
     * is read from chain and downloaded; from then on it is served without chain.
     */
    suspend fun resolveToLocalUri(dotNsName: String): Result<Uri>

    /**
     * Reads the build [dotNsName] points to on chain and downloads it when it is not the one served, without
     * touching what is served. True when a build other than the served one is on the device, ready for
     * [applyUpdate]. A name with no build on the device has nothing to update: its first load is the latest build.
     */
    suspend fun fetchUpdate(dotNsName: String): Result<Boolean>

    /** Serves the downloaded update of [dotNsName] from the next request on and deletes the build it replaces. */
    suspend fun applyUpdate(dotNsName: String)

    /** Names with a downloaded update awaiting [applyUpdate]. */
    fun observePendingUpdates(): Flow<Set<String>>

    /** Names with a build on the device. */
    fun namesOnDevice(): Set<String>

    suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?>

    /** Observes resolution progress for [dotNsName]. Emits [DotNsLoadProgress.Idle] when nothing is in flight. */
    fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress>

    suspend fun clearCache()
}

suspend fun DotNsResolver.resolveToLocalFile(dotNsName: String): Result<File> {
    return resolveToLocalUri(dotNsName).map { File(it.path!!) }
}
