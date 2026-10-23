package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapNotNull
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuild
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuildStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import java.io.File
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

private typealias DomainName = String

@OptIn(ExperimentalStdlibApi::class)
internal class RealDotNsResolver @Inject constructor(
    private val contractApi: DotNsContractApi,
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val buildStorage: DotNsBuildStorage,
    private val contentHashOverrides: ContentHashOverrides,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver {
    private val progressRegistry = DotNsLoadProgressRegistry()

    // A page's requests all land on one build, and a name's first load downloads once however many of them wait on it.
    private val buildLocks = KeyedMutex<DomainName>()

    // Two names can point at one build, which must not be unpacked twice into the same directory.
    private val downloadLocks = KeyedMutex<ContentHash>()

    override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> = withContext(dispatchers.computation) {
        buildLocks.withLock(dotNsName) {
            servedDirectory(dotNsName)?.let { directory ->
                progressRegistry.markCompleted(dotNsName)
                return@withLock Result.success(directory)
            }

            // Nothing on the device yet, so chain is the only source. Once downloaded, the build is served without it.
            readContentHash(dotNsName).flatMap { contentHash ->
                contentDirectory(contentHash, reportedAs = dotNsName)
                    .onSuccess { buildStorage.put(dotNsName, DotNsBuild(served = contentHash, update = null)) }
            }
        }
            .map(Uri::fromFile)
            .logFailure("failed to resolve $dotNsName")
            // A host screen reads a failure as final, and a domain with nothing registered would otherwise leave it waiting for good.
            .onFailure { progressRegistry.markFailed(dotNsName, it) }
    }

    override suspend fun fetchUpdate(dotNsName: String): Result<Boolean> = withContext(dispatchers.computation) {
        if (buildStorage.get(dotNsName) == null) return@withContext Result.success(false)

        // Read before taking any lock, so the round trip to chain holds up no page being served.
        val latest = withTimeoutOrNull(CHAIN_READ_TIMEOUT) { readContentHash(dotNsName) }
            ?: Result.failure(IllegalStateException("Chain did not answer for $dotNsName within $CHAIN_READ_TIMEOUT"))

        latest
            .flatMap { contentHash -> contentDirectory(contentHash, reportedAs = null).map { contentHash } }
            .map { contentHash -> recordUpdate(dotNsName, contentHash) }
            .logFailure("failed to fetch an update for $dotNsName")
    }

    override suspend fun applyUpdate(dotNsName: String) = withContext(dispatchers.computation) {
        buildLocks.withLock(dotNsName) {
            val build = buildStorage.get(dotNsName) ?: return@withLock
            val update = build.update ?: return@withLock

            buildStorage.put(dotNsName, DotNsBuild(served = update, update = null))
            Timber.d("$dotNsName now serves $update")
            deleteUnlessReferenced(build.served)
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

    override suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?> {
        return withContext(dispatchers.computation) {
            contractApi.getMetadata(dotNsName, key)
        }
    }

    override suspend fun clearCache() {
        buildStorage.clear()
        progressRegistry.clear()

        contentStorage.deleteAll()
    }

    /** Remembers [latest] as the update of [dotNsName], or drops a pending one when chain points back at the served build. */
    private suspend fun recordUpdate(dotNsName: DomainName, latest: ContentHash): Boolean = buildLocks.withLock(dotNsName) {
        val build = buildStorage.get(dotNsName) ?: return@withLock false
        val update = latest.takeIf { it != build.served }

        if (build.update != update) {
            buildStorage.put(dotNsName, build.copy(update = update))
            build.update?.let { deleteUnlessReferenced(it) }
        }

        update != null
    }

    // Two names may share one build, as a name and its alias do.
    private suspend fun deleteUnlessReferenced(contentHash: ContentHash) {
        val referenced = buildStorage.all().values.any { it.served == contentHash || it.update == contentHash }
        if (!referenced) contentStorage.deleteContent(contentHash)
    }

    private fun servedDirectory(dotNsName: DomainName): File? {
        return buildStorage.get(dotNsName)?.let { contentStorage.getContentDirectory(it.served) }
    }

    private suspend fun readContentHash(domainName: DomainName): Result<ContentHash> {
        Timber.d("resolving content hash for $domainName")

        return resolveContentHash(domainName)
            .logFailure("failed to resolve content hash for $domainName")
            .requireNotNull { IllegalStateException("Domain $domainName is not registered or has no content") }
    }

    private suspend fun resolveContentHash(domainName: DomainName): Result<ContentHash?> {
        val override = contentHashOverrides.getContentHashOverride(domainName)
        if (override != null) return Result.success(override)

        return contractApi.resolveContentHash(domainName)
            .mapNotNull { it.toHexString() }
    }

    /** The unpacked [contentHashHex], downloaded first when the device does not have it. */
    private suspend fun contentDirectory(contentHashHex: ContentHash, reportedAs: DomainName?): Result<File> {
        return downloadLocks.withLock(contentHashHex) {
            contentStorage.getContentDirectory(contentHashHex)?.let { directory ->
                reportedAs?.let(progressRegistry::markCompleted)
                return@withLock Result.success(directory)
            }

            download(contentHashHex, reportedAs)
        }
    }

    // Progress goes to the name in [reportedAs]; a download for an update passes none and stays off the host screen's bar.
    private suspend fun download(contentHashHex: ContentHash, reportedAs: DomainName?): Result<File> {
        Timber.d("content $contentHashHex not on the device, fetching CAR")
        reportedAs?.let(progressRegistry::markResolving)

        return carFetcher.fetchCarToFile(
            contentHash = contentHashHex.fromHex(),
            onProgress = { downloaded, total -> reportedAs?.let { progressRegistry.markDownloadProgress(it, downloaded, total) } }
        )
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

    private companion object {
        val CHAIN_READ_TIMEOUT = 30.seconds
    }
}
