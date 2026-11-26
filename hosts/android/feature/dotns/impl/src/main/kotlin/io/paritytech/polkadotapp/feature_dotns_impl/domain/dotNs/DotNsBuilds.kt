package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameStorage
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
    private val nameStorage: DotNsNameStorage,
    private val contentStorage: DotNsContentStorage,
) {
    private val lock = Mutex()

    private val lastChainReads = ConcurrentHashMap<String, TimeMark>()

    fun names(): Set<String> = nameStorage.all().keys

    fun served(dotNsName: String): DotNsNameVersion? = nameStorage.get(dotNsName)?.served

    fun servedDirectory(dotNsName: String): File? {
        return served(dotNsName)?.contentHash?.let(contentStorage::getContentDirectory)
    }

    fun observePending(): Flow<Set<String>> {
        return nameStorage.observeAll()
            .map { storedNames -> storedNames.filterValues { it.update != null }.keys }
            .distinctUntilChanged()
    }

    fun wasReadFromChainRecently(dotNsName: String): Boolean {
        return lastChainReads[dotNsName]?.let { it.elapsedNow() < MIN_TIME_BETWEEN_CHAIN_READS } == true
    }

    suspend fun changeServed(dotNsName: String, change: (DotNsNameVersion) -> DotNsNameVersion) = lock.withLock {
        markReadFromChain(dotNsName)
        val storedName = nameStorage.get(dotNsName)
            ?: StoredDotNsName(served = DotNsNameVersion(contentHash = null, records = emptyMap()), update = null)

        nameStorage.putAll(mapOf(dotNsName to storedName.copy(served = change(storedName.served))))
    }

    suspend fun recordLatest(latest: Map<String, DotNsNameChange>): Set<String> = lock.withLock {
        latest.keys.forEach(::markReadFromChain)
        val storedNames = latest.mapNotNull { (name, change) ->
            nameStorage.get(name)?.let { storedName -> name to storedName.copy(update = change.comparedTo(storedName.served)) }
        }.toMap()
        val replacedUpdates = latest.keys.mapNotNull { nameStorage.get(it)?.update?.contentHash }

        nameStorage.putAll(storedNames)
        (replacedUpdates + latest.values.mapNotNull { it.contentHash }).forEach { deleteUnlessReferenced(it) }

        storedNames.filterValues { it.update != null }.keys
    }

    suspend fun applyUpdates(dotNsNames: Collection<String>): Set<String> = lock.withLock {
        val pending = dotNsNames.mapNotNull { name ->
            nameStorage.get(name)?.takeIf { it.update != null }?.let { name to it }
        }.toMap()
        if (pending.isEmpty()) return@withLock emptySet()

        nameStorage.putAll(pending.mapValues { (_, storedName) -> storedName.applied() })
        Timber.d("${pending.keys} now serve their updates")

        pending.values
            .filter { it.update?.contentHash != null }
            .mapNotNull { it.served.contentHash }
            .forEach { deleteUnlessReferenced(it) }

        pending.keys
    }

    suspend fun deleteUnreferenced(contentHashes: Collection<ContentHash>) = lock.withLock {
        contentHashes.forEach { deleteUnlessReferenced(it) }
    }

    suspend fun deleteAllUnreferencedContent() = lock.withLock {
        contentStorage.deleteAllExcept(referencedContentHashes())
    }

    suspend fun clear() {
        nameStorage.clear()
        contentStorage.deleteAll()
    }

    private fun markReadFromChain(dotNsName: String) {
        lastChainReads[dotNsName] = TimeSource.Monotonic.markNow()
    }

    private suspend fun deleteUnlessReferenced(contentHash: ContentHash) {
        if (contentHash !in referencedContentHashes()) contentStorage.deleteContent(contentHash)
    }

    private fun referencedContentHashes(): Set<ContentHash> {
        return nameStorage.all().values.flatMapTo(HashSet()) { listOfNotNull(it.served.contentHash, it.update?.contentHash) }
    }

    private companion object {
        val MIN_TIME_BETWEEN_CHAIN_READS = 1.minutes
    }
}
