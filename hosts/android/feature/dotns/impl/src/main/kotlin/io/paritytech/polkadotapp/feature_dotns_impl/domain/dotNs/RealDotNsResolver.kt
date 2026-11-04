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
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuild
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuildStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameChange
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameVersion
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import timber.log.Timber
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes
import kotlin.time.TimeMark
import kotlin.time.TimeSource

private typealias DomainName = String

internal class RealDotNsResolver @Inject constructor(
    private val chainReader: DotNsChainReader,
    private val contentDownloader: DotNsContentDownloader,
    private val progressRegistry: DotNsLoadProgressRegistry,
    private val contentStorage: DotNsContentStorage,
    private val buildStorage: DotNsBuildStorage,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver {
    private val firstLoadLocks = KeyedMutex<DomainName>()

    private val buildsLock = Mutex()

    private val lastChainReads = ConcurrentHashMap<DomainName, TimeMark>()

    override suspend fun resolveToLocalUri(
        dotNsName: String,
        retryPolicy: NetworkRetryPolicy,
    ): Result<Uri> = withContext(dispatchers.computation) {
        firstLoadLocks.withLock(dotNsName) {
            servedDirectory(dotNsName)?.let { directory ->
                progressRegistry.markCompleted(dotNsName)
                return@withLock Result.success(directory)
            }

            chainReader.readContentHash(dotNsName, retryPolicy).flatMap { contentHash ->
                contentDownloader.download(contentHash, retryPolicy, progressDomain = dotNsName)
                    .onSuccess { changeServed(dotNsName) { served -> served.copy(contentHash = contentHash) } }
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
        val servedRecords = buildStorage.get(dotNsName)?.served?.records.orEmpty()
        if (key in servedRecords) return@withContext Result.success(servedRecords[key])

        chainReader.readRecord(dotNsName, key, retryPolicy)
            .onSuccess { value -> changeServed(dotNsName) { it.copy(records = it.records + (key to value)) } }
    }

    override suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Boolean> = withContext(dispatchers.computation) {
        val served = dotNsNames.filterNot(::readRecently).mapNotNull { name -> buildStorage.get(name)?.let { name to it.served } }
        if (served.isEmpty()) return@withContext Result.success(false)

        served.mapAsync { (name, version) -> readLatest(name, version).map { name to it } }
            .flattenResult()
            .map { latest -> recordUpdates(latest.toMap()) }
            .logFailure("failed to fetch updates for $dotNsNames")
    }

    override suspend fun applyUpdates(dotNsNames: Collection<String>) = withContext(dispatchers.computation) {
        buildsLock.withLock {
            val pending = dotNsNames.mapNotNull { name ->
                buildStorage.get(name)?.takeIf { it.update != null }?.let { name to it }
            }.toMap()
            if (pending.isEmpty()) return@withLock

            buildStorage.putAll(pending.mapValues { (_, build) -> build.applied() })
            Timber.d("${pending.keys} now serve their updates")

            pending.values
                .filter { it.update?.contentHash != null }
                .mapNotNull { it.served.contentHash }
                .forEach { deleteUnlessReferenced(it) }
        }
    }

    override fun observePendingUpdates(): Flow<Set<String>> {
        return buildStorage.observeAll()
            .map { builds -> builds.filterValues { it.update != null }.keys }
            .distinctUntilChanged()
    }

    override fun namesOnDevice(): Set<String> = buildStorage.all().keys

    override fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress> {
        return progressRegistry.observe(dotNsName)
    }

    override suspend fun clearCache() {
        buildStorage.clear()
        progressRegistry.clear()

        contentStorage.deleteAll()
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

    private suspend fun recordUpdates(latest: Map<DomainName, DotNsNameChange>): Boolean = buildsLock.withLock {
        latest.keys.forEach(::markReadFromChain)
        val builds = latest.mapNotNull { (name, change) ->
            buildStorage.get(name)?.let { build -> name to build.copy(update = change.comparedTo(build.served)) }
        }.toMap()
        val replacedUpdates = latest.keys.mapNotNull { buildStorage.get(it)?.update?.contentHash }

        buildStorage.putAll(builds)
        (replacedUpdates + latest.values.mapNotNull { it.contentHash }).forEach { deleteUnlessReferenced(it) }

        builds.values.any { it.update != null }
    }

    private suspend fun changeServed(dotNsName: DomainName, change: (DotNsNameVersion) -> DotNsNameVersion) = buildsLock.withLock {
        markReadFromChain(dotNsName)
        val build = buildStorage.get(dotNsName)
            ?: DotNsBuild(served = DotNsNameVersion(contentHash = null, records = emptyMap()), update = null)

        buildStorage.putAll(mapOf(dotNsName to build.copy(served = change(build.served))))
    }

    private fun markReadFromChain(dotNsName: DomainName) {
        lastChainReads[dotNsName] = TimeSource.Monotonic.markNow()
    }

    private fun readRecently(dotNsName: DomainName): Boolean {
        return lastChainReads[dotNsName]?.let { it.elapsedNow() < MIN_TIME_BETWEEN_CHAIN_READS } == true
    }

    private suspend fun deleteUnlessReferenced(contentHash: ContentHash) {
        val referenced = buildStorage.all().values.any { it.served.contentHash == contentHash || it.update?.contentHash == contentHash }
        if (!referenced) contentStorage.deleteContent(contentHash)
    }

    private fun servedDirectory(dotNsName: DomainName): File? {
        return buildStorage.get(dotNsName)?.served?.contentHash?.let(contentStorage::getContentDirectory)
    }

    private companion object {
        val MIN_TIME_BETWEEN_CHAIN_READS = 1.minutes
    }
}
