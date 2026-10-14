package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.memory.MapCache
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
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
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject

private typealias DomainName = String

@OptIn(ExperimentalStdlibApi::class)
internal class RealDotNsResolver @Inject constructor(
    private val contractApi: DotNsContractApi,
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val contentHashOverrides: ContentHashOverrides,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver, CoroutineScope {
    override val coroutineContext = dispatchers.io + SupervisorJob()

    private val progressRegistry = DotNsLoadProgressRegistry()

    private val localDomainsCache: MapCache<DomainName, Result<Uri>> = MapCache(this) { domainName ->
        Timber.d("resolving content hash for $domainName")

        resolveContentHash(domainName)
            .logFailure("failed to resolve content hash for $domainName")
            .requireNotNull { IllegalStateException("Domain $domainName is not registered or has no content") }
            .flatMap { contentHash -> resolveContentHash(domainName, contentHash) }
            .logFailure("failed to resolve $domainName")
            // A domain with nothing registered behind it never reaches the download that reports its
            // own failures, and a host screen reading progress would wait on it for good.
            .onFailure { progressRegistry.markFailed(domainName, it) }
    }

    // TODO v1: Currently always fetches the latest content hash from chain for session. In the target solution,
    //  product updates should be under full user control: once a content hash is fetched for a domain,
    //  it should be pinned and never auto-updated. A separate UX flow will prompt the user to
    //  acknowledge and update to a newer version when available.
    override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> = withContext(dispatchers.computation) {
        localDomainsCache.getOrCompute(dotNsName)
    }

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
            .onFailure { progressRegistry.markFailed(dotNsName, it) }
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
        localDomainsCache.clear()
        progressRegistry.clear()

        contentStorage.deleteAll()
    }
}
