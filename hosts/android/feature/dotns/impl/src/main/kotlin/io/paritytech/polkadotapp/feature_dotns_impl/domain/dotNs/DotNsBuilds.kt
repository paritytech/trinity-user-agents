package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

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
import timber.log.Timber
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.minutes
import kotlin.time.TimeMark
import kotlin.time.TimeSource

@Singleton
internal class DotNsBuilds @Inject constructor(
    private val buildStorage: DotNsBuildStorage,
    private val contentStorage: DotNsContentStorage,
) {
    private val lock = Mutex()

    private val lastChainReads = ConcurrentHashMap<String, TimeMark>()

    fun names(): Set<String> = buildStorage.all().keys

    fun served(dotNsName: String): DotNsNameVersion? = buildStorage.get(dotNsName)?.served

    fun servedDirectory(dotNsName: String): File? {
        return served(dotNsName)?.contentHash?.let(contentStorage::getContentDirectory)
    }

    fun observePending(): Flow<Set<String>> {
        return buildStorage.observeAll()
            .map { builds -> builds.filterValues { it.update != null }.keys }
            .distinctUntilChanged()
    }

    fun wasReadFromChainRecently(dotNsName: String): Boolean {
        return lastChainReads[dotNsName]?.let { it.elapsedNow() < MIN_TIME_BETWEEN_CHAIN_READS } == true
    }

    suspend fun changeServed(dotNsName: String, change: (DotNsNameVersion) -> DotNsNameVersion) = lock.withLock {
        markReadFromChain(dotNsName)
        val build = buildStorage.get(dotNsName)
            ?: DotNsBuild(served = DotNsNameVersion(contentHash = null, records = emptyMap()), update = null)

        buildStorage.putAll(mapOf(dotNsName to build.copy(served = change(build.served))))
    }

    suspend fun recordLatest(latest: Map<String, DotNsNameChange>): Set<String> = lock.withLock {
        latest.keys.forEach(::markReadFromChain)
        val builds = latest.mapNotNull { (name, change) ->
            buildStorage.get(name)?.let { build -> name to build.copy(update = change.comparedTo(build.served)) }
        }.toMap()
        val replacedUpdates = latest.keys.mapNotNull { buildStorage.get(it)?.update?.contentHash }

        buildStorage.putAll(builds)
        (replacedUpdates + latest.values.mapNotNull { it.contentHash }).forEach { deleteUnlessReferenced(it) }

        builds.filterValues { it.update != null }.keys
    }

    suspend fun applyUpdates(dotNsNames: Collection<String>) = lock.withLock {
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

    suspend fun clear() {
        buildStorage.clear()
        contentStorage.deleteAll()
    }

    private fun markReadFromChain(dotNsName: String) {
        lastChainReads[dotNsName] = TimeSource.Monotonic.markNow()
    }

    private suspend fun deleteUnlessReferenced(contentHash: ContentHash) {
        val referenced = buildStorage.all().values.any { it.served.contentHash == contentHash || it.update?.contentHash == contentHash }
        if (!referenced) contentStorage.deleteContent(contentHash)
    }

    private companion object {
        val MIN_TIME_BETWEEN_CHAIN_READS = 1.minutes
    }
}
