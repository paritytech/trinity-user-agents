package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsBuildStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class DotNsBuilds @Inject constructor(
    private val buildStorage: DotNsBuildStorage,
    private val contentStorage: DotNsContentStorage,
) {
    private val lock = Mutex()

    fun served(dotNsName: String): DotNsNameVersion? = buildStorage.get(dotNsName)

    fun servedDirectory(dotNsName: String): File? {
        return served(dotNsName)?.contentHash?.let(contentStorage::getContentDirectory)
    }

    suspend fun changeServed(dotNsName: String, change: (DotNsNameVersion) -> DotNsNameVersion) = lock.withLock {
        val served = buildStorage.get(dotNsName) ?: DotNsNameVersion(contentHash = null, records = emptyMap())
        buildStorage.put(dotNsName, change(served))
    }

    suspend fun clear() {
        buildStorage.clear()
        contentStorage.deleteAll()
    }
}
