package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameChange
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameVersion
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import javax.inject.Inject

private typealias DomainName = String

internal class RealDotNsResolver @Inject constructor(
    private val chainReader: DotNsChainReader,
    private val contentDownloader: DotNsContentDownloader,
    private val builds: DotNsBuilds,
    private val progressRegistry: DotNsLoadProgressRegistry,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver {
    private val firstLoadLocks = KeyedMutex<DomainName>()

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

    override suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Boolean> = withContext(dispatchers.computation) {
        val served = dotNsNames.filterNot(builds::wasReadFromChainRecently).mapNotNull { name -> builds.served(name)?.let { name to it } }
        if (served.isEmpty()) return@withContext Result.success(false)

        served.mapAsync { (name, version) -> readLatest(name, version).map { name to it } }
            .flattenResult()
            .map { latest -> builds.recordLatest(latest.toMap()) }
            .logFailure("failed to fetch updates for $dotNsNames")
    }

    override suspend fun applyUpdates(dotNsNames: Collection<String>) = withContext(dispatchers.computation) {
        builds.applyUpdates(dotNsNames)
    }

    override fun observePendingUpdates(): Flow<Set<String>> = builds.observePending()

    override fun namesOnDevice(): Set<String> = builds.names()

    override fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress> {
        return progressRegistry.observe(dotNsName)
    }

    override suspend fun clearCache() {
        builds.clear()
        progressRegistry.clear()
    }

    private suspend fun readLatest(dotNsName: DomainName, served: DotNsNameVersion): Result<DotNsNameChange> {
        val contentHash = if (served.contentHash == null) {
            null
        } else {
            downloadLatestContent(dotNsName).getOrElse { return Result.failure(it) }
        }

        return served.records.keys
            .mapAsync { key -> chainReader.readRecord(dotNsName, key, NetworkRetryPolicy.DEFAULT).map { key to it } }
            .flattenResult()
            .map { records -> DotNsNameChange(contentHash = contentHash, records = records.toMap()) }
    }

    private suspend fun downloadLatestContent(dotNsName: DomainName): Result<ContentHash> {
        return chainReader.readContentHash(dotNsName, NetworkRetryPolicy.DEFAULT).flatMap { contentHash ->
            contentDownloader.download(contentHash, NetworkRetryPolicy.DEFAULT, progressDomain = null).map { contentHash }
        }
    }
}
