package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import kotlinx.serialization.Serializable

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
data class StoredDotNsName(val served: DotNsNameVersion, val update: DotNsNameChange?) {
    fun applied(): StoredDotNsName = StoredDotNsName(served = update?.let(served::with) ?: served, update = null)
}
