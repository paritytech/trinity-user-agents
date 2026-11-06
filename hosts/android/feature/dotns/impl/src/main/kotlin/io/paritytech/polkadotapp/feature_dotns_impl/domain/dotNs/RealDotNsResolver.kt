package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class RealDotNsResolver @Inject constructor(
    private val chainReader: DotNsChainReader,
    private val contentDownloader: DotNsContentDownloader,
    private val builds: DotNsBuilds,
    private val progressRegistry: DotNsLoadProgressRegistry,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver {
    private val firstLoadLocks = KeyedMutex<String>()

    override suspend fun resolveToLocalUri(
        dotNsName: String,
        retryPolicy: NetworkRetryPolicy,
    ): Result<Uri> = withContext(dispatchers.computation) {
        firstLoadLocks.withLock(dotNsName) {
            builds.servedDirectory(dotNsName)?.let { directory ->
                progressRegistry.markCompleted(dotNsName)
                return@withLock Result.success(directory)
            }

            chainReader.readContentHash(dotNsName, retryPolicy).flatMap { contentHash ->
                contentDownloader.download(contentHash, retryPolicy, progressDomain = dotNsName)
                    .onSuccess { builds.changeServed(dotNsName) { served -> served.copy(contentHash = contentHash) } }
            }
        }
            .map(Uri::fromFile)
            .logFailure("failed to resolve $dotNsName")
            .onFailure { progressRegistry.markFailed(dotNsName, it) }
    }

    override suspend fun getMetadataEntry(
        dotNsName: String,
        key: String,
        retryPolicy: NetworkRetryPolicy,
    ): Result<String?> = withContext(dispatchers.computation) {
        val servedRecords = builds.served(dotNsName)?.records.orEmpty()
        if (key in servedRecords) return@withContext Result.success(servedRecords[key])

        chainReader.readRecord(dotNsName, key, retryPolicy)
            .onSuccess { value -> builds.changeServed(dotNsName) { it.copy(records = it.records + (key to value)) } }
    }

    override fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress> {
        return progressRegistry.observe(dotNsName)
    }

    override suspend fun clearCache() {
        builds.clear()
        progressRegistry.clear()
    }
}
