package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.Serializable

// The content hash stays null until the device loads the name's content.
@Serializable
data class DotNsNameVersion(val contentHash: ContentHash?, val records: Map<String, String?>) {
    fun with(change: DotNsNameChange): DotNsNameVersion {
        return DotNsNameVersion(contentHash = change.contentHash ?: contentHash, records = records + change.records)
    }
}

// Holds only new values: a null content hash means the content did not change.
@Serializable
data class DotNsNameChange(val contentHash: ContentHash?, val records: Map<String, String?>) {
    fun comparedTo(served: DotNsNameVersion): DotNsNameChange? {
        val change = DotNsNameChange(
            contentHash = contentHash.takeIf { it != served.contentHash },
            records = records.filter { (key, value) -> served.records[key] != value },
        )
        return change.takeIf { it.contentHash != null || it.records.isNotEmpty() }
    }
}

@Serializable
data class DotNsBuild(val served: DotNsNameVersion, val update: DotNsNameChange?) {
    fun applied(): DotNsBuild = DotNsBuild(served = update?.let(served::with) ?: served, update = null)
}

interface DotNsBuildStorage {
    fun get(dotNsName: String): DotNsBuild?

    // One edit, so an update of several names is kept for all of them or for none.
    fun putAll(builds: Map<String, DotNsBuild>)

    fun all(): Map<String, DotNsBuild>

    fun observeAll(): Flow<Map<String, DotNsBuild>>

    fun clear()
}
