package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.MapCache
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import javax.inject.Inject

private typealias DomainName = String

internal class RealDotNsResolver @Inject constructor(
    private val chainReader: DotNsChainReader,
    private val contentDownloader: DotNsContentDownloader,
    private val contentStorage: DotNsContentStorage,
    private val progressRegistry: DotNsLoadProgressRegistry,
    private val dispatchers: CoroutineDispatchers
) : DotNsResolver, CoroutineScope {
    override val coroutineContext = dispatchers.io + SupervisorJob()

    private val localDomainsCache: MapCache<DomainName, Result<Uri>> = MapCache(this) { domainName ->
        chainReader.readContentHash(domainName)
            .flatMap { contentHash -> contentDownloader.download(contentHash, progressDomain = domainName) }
            .map(Uri::fromFile)
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
            // Usually a read that did not land; remembered, it would outlast the outage until restart.
            .onFailure { localDomainsCache.remove(dotNsName) }
    }

    override fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress> {
        return progressRegistry.observe(dotNsName)
    }

    override suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?> {
        return withContext(dispatchers.computation) {
            chainReader.readRecord(dotNsName, key)
        }
    }

    override suspend fun clearCache() {
        localDomainsCache.clear()
        progressRegistry.clear()

        contentStorage.deleteAll()
    }
}
