package io.paritytech.polkadotapp.feature_dotns_api.domain

import android.net.Uri
import kotlinx.coroutines.flow.Flow
import java.io.File

interface DotNsResolver {
    /** The build the app serves for [dotNsName]. Resolved from chain on first use, then kept until [updateToLatest]. */
    suspend fun resolveToLocalUri(dotNsName: String): Result<Uri>

    /**
     * Serves [dotNsName] from the build chain points it at from now on. When chain or the download fails, the build
     * already served stays, and a build that failed to download is not reported by [hasNewBuild].
     */
    suspend fun updateToLatest(dotNsName: String): Result<Uri>

    /** Whether chain points [dotNsName] at a build other than the one served. Downloads nothing. */
    suspend fun hasNewBuild(dotNsName: String): Result<Boolean>

    suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?>

    /** Observes resolution progress for [dotNsName]. Emits [DotNsLoadProgress.Idle] when nothing is in flight. */
    fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress>

    suspend fun clearCache()
}

suspend fun DotNsResolver.resolveToLocalFile(dotNsName: String): Result<File> {
    return resolveToLocalUri(dotNsName).map { File(it.path!!) }
}
