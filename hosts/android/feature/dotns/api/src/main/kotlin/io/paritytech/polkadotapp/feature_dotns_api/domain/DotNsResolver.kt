package io.paritytech.polkadotapp.feature_dotns_api.domain

import android.net.Uri
import kotlinx.coroutines.flow.Flow
import java.io.File

interface DotNsResolver {
    suspend fun resolveToLocalUri(dotNsName: String): Result<Uri>

    suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?>

    /** Observes resolution progress for [dotNsName]. Emits [DotNsLoadProgress.Idle] when nothing is in flight. */
    fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress>

    suspend fun clearCache()
}

suspend fun DotNsResolver.resolveToLocalFile(dotNsName: String): Result<File> {
    return resolveToLocalUri(dotNsName).map { File(it.path!!) }
}
