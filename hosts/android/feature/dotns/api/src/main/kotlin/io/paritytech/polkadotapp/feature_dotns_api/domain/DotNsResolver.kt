package io.paritytech.polkadotapp.feature_dotns_api.domain

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import kotlinx.coroutines.flow.Flow
import java.io.File

interface DotNsResolver {
    suspend fun resolveToLocalUri(
        dotNsName: String,
        retryPolicy: NetworkRetryPolicy = NetworkRetryPolicy.DEFAULT,
    ): Result<Uri>

    suspend fun getMetadataEntry(
        dotNsName: String,
        key: String,
        retryPolicy: NetworkRetryPolicy = NetworkRetryPolicy.DEFAULT,
    ): Result<String?>

    suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Boolean>

    suspend fun applyUpdates(dotNsNames: Collection<String>)

    fun observePendingUpdates(): Flow<Set<String>>

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
