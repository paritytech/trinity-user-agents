package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.network.binding.DispatchError
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.common.utils.mapNotNull
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.retrying
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.IpfsHttpException
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuild
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuildStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameChange
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameVersion
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractReverted
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeoutException
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeMark
import kotlin.time.TimeSource

private typealias DomainName = String

@OptIn(ExperimentalStdlibApi::class)
internal class RealDotNsResolver @Inject constructor(
    private val contractApi: DotNsContractApi,
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val buildStorage: DotNsBuildStorage,
    private val contentHashOverrides: ContentHashOverrides,
    private val networkStateService: NetworkStateService,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver {
    private val progressRegistry = DotNsLoadProgressRegistry()

    // A name's first load downloads once however many requests wait on it.
    private val firstLoadLocks = KeyedMutex<DomainName>()

    // Two names can point at one build, which must not be unpacked twice into the same directory.
    private val downloadLocks = KeyedMutex<ContentHash>()

    // Every change to what the names serve goes through here, so an update lands on all of its names at once.
    private val buildsLock = Mutex()

    // A name read moments ago, by its first load or by another check, is already the latest; an update check skips it.
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

            // Nothing on the device yet, so chain is the only source. Once downloaded, the content is served without it.
            readContentHash(dotNsName, retryPolicy).flatMap { contentHash ->
                contentDirectory(contentHash, reportedAs = dotNsName, retryPolicy)
                    .onSuccess { changeServed(dotNsName) { it.copy(contentHash = contentHash) } }
            }
        }
            .map(Uri::fromFile)
            .logFailure("failed to resolve $dotNsName")
            // A host screen reads a failure as final, and a domain with nothing registered would otherwise leave it waiting for good.
            .onFailure { progressRegistry.markFailed(dotNsName, it) }
    }

    override suspend fun getMetadataEntry(
        dotNsName: String,
        key: String,
        retryPolicy: NetworkRetryPolicy,
    ): Result<String?> = withContext(dispatchers.computation) {
        val servedRecords = buildStorage.get(dotNsName)?.served?.records.orEmpty()
        if (key in servedRecords) return@withContext Result.success(servedRecords[key])

        readOnChain(retryPolicy) { contractApi.getMetadata(dotNsName, key) }
            .onSuccess { value -> changeServed(dotNsName) { it.copy(records = it.records + (key to value)) } }
    }

    override suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Boolean> = withContext(dispatchers.computation) {
        val served = dotNsNames.filterNot(::readRecently).mapNotNull { name -> buildStorage.get(name)?.let { name to it.served } }
        if (served.isEmpty()) return@withContext Result.success(false)

        // Read before taking any lock, so the round trips to chain hold up no page being served.
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
        // Content the device never loaded stays out of the update: its first load reads the latest anyway.
        val contentHash = if (served.contentHash == null) {
            null
        } else {
            downloadLatestContent(dotNsName).getOrElse { return Result.failure(it) }
        }

        return served.records.keys
            .mapAsync { key -> readOnChain(NetworkRetryPolicy.DEFAULT) { contractApi.getMetadata(dotNsName, key) }.map { key to it } }
            .flattenResult()
            .map { records -> DotNsNameChange(contentHash = contentHash, records = records.toMap()) }
    }

    private suspend fun downloadLatestContent(dotNsName: DomainName): Result<ContentHash> {
        return readContentHash(dotNsName, NetworkRetryPolicy.DEFAULT).flatMap { contentHash ->
            contentDirectory(contentHash, reportedAs = null, NetworkRetryPolicy.DEFAULT).map { contentHash }
        }
    }

    // What differs from the served version becomes each name's update; a pending one chain no longer points at is dropped.
    private suspend fun recordUpdates(latest: Map<DomainName, DotNsNameChange>): Boolean = buildsLock.withLock {
        latest.keys.forEach(::markReadFromChain)
        val builds = latest.mapNotNull { (name, change) ->
            buildStorage.get(name)?.let { build -> name to build.copy(update = change.comparedTo(build.served)) }
        }.toMap()
        val replacedUpdates = latest.keys.mapNotNull { buildStorage.get(it)?.update?.contentHash }

        buildStorage.putAll(builds)
        // A download that turned out to be no update, and an update replaced by a newer one, are left unreferenced.
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

    // Two names may share one build, as a name and its alias do.
    private suspend fun deleteUnlessReferenced(contentHash: ContentHash) {
        val referenced = buildStorage.all().values.any { it.served.contentHash == contentHash || it.update?.contentHash == contentHash }
        if (!referenced) contentStorage.deleteContent(contentHash)
    }

    private fun servedDirectory(dotNsName: DomainName): File? {
        return buildStorage.get(dotNsName)?.served?.contentHash?.let(contentStorage::getContentDirectory)
    }

    private suspend fun readContentHash(domainName: DomainName, retryPolicy: NetworkRetryPolicy): Result<ContentHash> {
        Timber.d("resolving content hash for $domainName")

        return resolveContentHash(domainName, retryPolicy)
            .logFailure("failed to resolve content hash for $domainName")
            .requireNotNull { IllegalStateException("Domain $domainName is not registered or has no content") }
    }

    private suspend fun resolveContentHash(domainName: DomainName, retryPolicy: NetworkRetryPolicy): Result<ContentHash?> {
        val override = contentHashOverrides.getContentHashOverride(domainName)
        if (override != null) return Result.success(override)

        return readOnChain(retryPolicy) { contractApi.resolveContentHash(domainName) }
            .mapNotNull { it.toHexString() }
    }

    // A node that never answers would hold the caller for good; a read cut off here is retried like a dropped one.
    private suspend fun <T> readOnChain(retryPolicy: NetworkRetryPolicy, read: suspend () -> Result<T>): Result<T> {
        return networkStateService.retrying(retryPolicy, ::isWorthRetrying) {
            withTimeoutOrNull(CHAIN_READ_TIMEOUT) { read() }
                ?: Result.failure(TimeoutException("Chain did not answer within $CHAIN_READ_TIMEOUT"))
        }
    }

    private suspend fun contentDirectory(
        contentHashHex: ContentHash,
        reportedAs: DomainName?,
        retryPolicy: NetworkRetryPolicy,
    ): Result<File> {
        return downloadLocks.withLock(contentHashHex) {
            contentStorage.getContentDirectory(contentHashHex)?.let { directory ->
                reportedAs?.let(progressRegistry::markCompleted)
                return@withLock Result.success(directory)
            }

            download(contentHashHex, reportedAs, retryPolicy)
        }
    }

    // Progress goes to the name in [reportedAs]; a download for an update passes none and stays off the host screen's bar.
    private suspend fun download(contentHashHex: ContentHash, reportedAs: DomainName?, retryPolicy: NetworkRetryPolicy): Result<File> {
        Timber.d("content $contentHashHex not on the device, fetching CAR")
        reportedAs?.let(progressRegistry::markResolving)

        return networkStateService.retrying(retryPolicy, ::isWorthRetrying) {
            carFetcher.fetchCarToFile(
                contentHash = contentHashHex.fromHex(),
                onProgress = { downloaded, total -> reportedAs?.let { progressRegistry.markDownloadProgress(it, downloaded, total) } }
            )
        }
            .mapCatching { carFile ->
                reportedAs?.let(progressRegistry::markUnpacking)
                try {
                    contentStorage.saveContentStreaming(contentHashHex) { writer ->
                        CarParser.unpack(carFile) { path, content -> writer.write(path, content) }
                    }
                    requireNotNull(contentStorage.getContentDirectory(contentHashHex)) { "nothing unpacked for $contentHashHex" }
                } finally {
                    carFile.delete()
                }
            }
            .onSuccess { reportedAs?.let(progressRegistry::markCompleted) }
            .logFailure("failed to download $contentHashHex")
    }

    // A contract that ran and refused, malformed data and a gateway answering "no such archive" say the same on the next attempt.
    private fun isWorthRetrying(error: Throwable): Boolean = when (error) {
        is ReviveContractReverted, is DispatchError, is IllegalArgumentException -> false
        is IpfsHttpException -> error.isTransient
        else -> true
    }

    private companion object {
        val CHAIN_READ_TIMEOUT = 30.seconds
        val MIN_TIME_BETWEEN_CHAIN_READS = 1.minutes
    }
}
