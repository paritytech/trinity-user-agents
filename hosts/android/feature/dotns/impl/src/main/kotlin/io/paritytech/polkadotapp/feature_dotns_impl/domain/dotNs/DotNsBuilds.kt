package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuildStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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

    fun wasReadFromChainRecently(dotNsName: String): Boolean {
        return lastChainReads[dotNsName]?.let { it.elapsedNow() < MIN_TIME_BETWEEN_CHAIN_READS } == true
    }

    suspend fun changeServed(dotNsName: String, change: (DotNsNameVersion) -> DotNsNameVersion) = lock.withLock {
        markReadFromChain(dotNsName)
        val build = buildStorage.get(dotNsName)
            ?: DotNsBuild(served = DotNsNameVersion(contentHash = null, records = emptyMap()), update = null)

        buildStorage.putAll(mapOf(dotNsName to build.copy(served = change(build.served))))
    }

    suspend fun recordLatest(latest: Map<String, DotNsNameChange>) = lock.withLock {
        latest.keys.forEach(::markReadFromChain)
        val builds = latest.mapNotNull { (name, change) ->
            buildStorage.get(name)?.let { build -> name to build.copy(update = change.comparedTo(build.served)) }
        }.toMap()
        val replacedUpdates = latest.keys.mapNotNull { buildStorage.get(it)?.update?.contentHash }

        buildStorage.putAll(builds)
        (replacedUpdates + latest.values.mapNotNull { it.contentHash }).forEach { deleteUnlessReferenced(it) }
    }

    suspend fun deleteUnreferenced(contentHashes: Collection<ContentHash>) = lock.withLock {
        contentHashes.forEach { deleteUnlessReferenced(it) }
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
