package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapNotNull
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject

private typealias DomainName = String

private class ResolvedContent(val contentHash: ContentHash, val uri: Uri)

@OptIn(ExperimentalStdlibApi::class)
internal class RealDotNsResolver @Inject constructor(
    private val contractApi: DotNsContractApi,
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val contentHashOverrides: ContentHashOverrides,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver {
    private val progressRegistry = DotNsLoadProgressRegistry()

    private val contentInUseMutex = Mutex()
    private val contentInUse = mutableMapOf<DomainName, ResolvedContent>()

    // A page whose new content failed to download is served its content in use, so offering that content as a new
    // build would only reload the page into the same one. Read without the lock, which a download may hold.
    private val unavailableContent = ConcurrentHashMap<DomainName, ContentHash>()

    override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> = withContext(dispatchers.computation) {
        contentInUseMutex.withLock {
            contentInUse[dotNsName]?.let { return@withLock Result.success(it.uri) }

            readContentHash(dotNsName)
                .flatMap { contentHash -> useContent(dotNsName, contentHash) }
                .reportFailure(dotNsName)
        }
    }

    override suspend fun resolveLatestToLocalUri(dotNsName: String): Result<Uri> = withContext(dispatchers.computation) {
        // Read before taking the lock, so the round trip to chain does not hold up the pages being served.
        val latestContentHash = readContentHash(dotNsName)

        contentInUseMutex.withLock {
            val inUse = contentInUse[dotNsName]

            latestContentHash
                .flatMap { contentHash ->
                    inUse?.takeIf { it.contentHash == contentHash }
                        ?.let { Result.success(it.uri) }
                        ?: useContent(dotNsName, contentHash).onFailure { unavailableContent[dotNsName] = contentHash }
                }
                .flatRecover { error -> inUse?.let { keepInUse(dotNsName, it, error) } ?: Result.failure(error) }
                .reportFailure(dotNsName)
        }
    }

    override suspend fun isOutdated(dotNsName: String, localUri: Uri): Result<Boolean> = withContext(dispatchers.computation) {
        readContentHash(dotNsName).map { contentHash ->
            contentHash != unavailableContent[dotNsName] &&
                contentStorage.getContentDirectory(contentHash)?.absolutePath != localUri.path
        }
    }

    private suspend fun readContentHash(domainName: DomainName): Result<ContentHash> {
        Timber.d("resolving content hash for $domainName")

        return resolveContentHash(domainName)
            .logFailure("failed to resolve content hash for $domainName")
            .requireNotNull { IllegalStateException("Domain $domainName is not registered or has no content") }
    }

    private suspend fun useContent(dotNsName: DomainName, contentHash: ContentHash): Result<Uri> {
        return resolveContentHash(dotNsName, contentHash)
            .onSuccess { uri ->
                contentInUse[dotNsName] = ResolvedContent(contentHash, uri)
                unavailableContent.remove(dotNsName)
            }
    }

    private fun keepInUse(dotNsName: DomainName, inUse: ResolvedContent, cause: Throwable): Result<Uri> {
        Timber.w(cause, "keeping content ${inUse.contentHash} for $dotNsName")
        progressRegistry.markCompleted(dotNsName)
        return Result.success(inUse.uri)
    }

    // Reported once the resolution is over, never from a step a fallback may still recover: a host screen reads
    // a failure as final, and a domain with nothing registered would otherwise leave it waiting for good.
    private fun Result<Uri>.reportFailure(dotNsName: DomainName): Result<Uri> =
        logFailure("failed to resolve $dotNsName")
            .onFailure { progressRegistry.markFailed(dotNsName, it) }

    private suspend fun resolveContentHash(domainName: DomainName): Result<ContentHash?> {
        val override = contentHashOverrides.getContentHashOverride(domainName)
        if (override != null) return Result.success(override)

        return contractApi.resolveContentHash(domainName)
            .mapNotNull { it.toHexString() }
    }

    private suspend fun resolveContentHash(dotNsName: String, contentHashHex: ContentHash): Result<Uri> {
        val contentHash = contentHashHex.fromHex()
        Timber.d("resolving content for content hash $contentHashHex ($dotNsName)")

        // Check if we already have this content
        val existingDir = contentStorage.getContentDirectory(contentHashHex)
        if (existingDir != null) {
            Timber.d("content $contentHashHex already cached on disk for $dotNsName")
            progressRegistry.markCompleted(dotNsName)
            return Result.success(Uri.fromFile(existingDir))
        }

        Timber.d("content $contentHashHex not cached — fetching CAR for $dotNsName")
        progressRegistry.markResolving(dotNsName)
        return carFetcher.fetchCarToFile(
            contentHash = contentHash,
            onProgress = { downloaded, total -> progressRegistry.markDownloadProgress(dotNsName, downloaded, total) }
        )
            .mapCatching { carFile ->
                progressRegistry.markUnpacking(dotNsName)
                try {
                    contentStorage.saveContentStreaming(contentHashHex) { writer ->
                        CarParser.unpack(carFile) { path, content -> writer.write(path, content) }
                    }
                    Timber.d("saved content $contentHashHex for $dotNsName")
                    Uri.fromFile(contentStorage.getContentDirectory(contentHashHex)!!)
                } finally {
                    carFile.delete()
                }
            }
            .onSuccess { progressRegistry.markCompleted(dotNsName) }
            .logFailure("failed to parse CAR for $contentHashHex")
    }

    override fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress> {
        return progressRegistry.observe(dotNsName)
    }

    override suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?> {
        return withContext(dispatchers.computation) {
            contractApi.getMetadata(dotNsName, key)
        }
    }

    override suspend fun clearCache() {
        contentInUseMutex.withLock { contentInUse.clear() }
        unavailableContent.clear()
        progressRegistry.clear()

        contentStorage.deleteAll()
    }
}
