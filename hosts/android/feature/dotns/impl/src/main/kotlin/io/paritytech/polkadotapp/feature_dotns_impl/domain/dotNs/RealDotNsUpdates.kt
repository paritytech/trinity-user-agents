package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUpdates
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameChange
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameVersion
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class RealDotNsUpdates @Inject constructor(
    private val chainReader: DotNsChainReader,
    private val contentDownloader: DotNsContentDownloader,
    private val builds: DotNsBuilds,
    private val dispatchers: CoroutineDispatchers,
) : DotNsUpdates {
    override suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Unit> = withContext(dispatchers.computation) {
        val served = dotNsNames.filterNot(builds::wasReadFromChainRecently).mapNotNull { name -> builds.served(name)?.let { name to it } }
        if (served.isEmpty()) return@withContext Result.success(Unit)

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

    private suspend fun readLatest(dotNsName: String, served: DotNsNameVersion): Result<DotNsNameChange> {
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

    private suspend fun downloadLatestContent(dotNsName: String): Result<ContentHash> {
        return chainReader.readContentHash(dotNsName, NetworkRetryPolicy.DEFAULT).flatMap { contentHash ->
            contentDownloader.download(contentHash, NetworkRetryPolicy.DEFAULT, progressDomain = null).map { contentHash }
        }
    }
}
