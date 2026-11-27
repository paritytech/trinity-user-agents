package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs.StoredDotNsName
import kotlinx.coroutines.flow.Flow

interface DotNsNameStorage {
    fun get(dotNsName: String): StoredDotNsName?

    fun putAll(names: Map<String, StoredDotNsName>)

    fun all(): Map<String, StoredDotNsName>

    fun observeAll(): Flow<Map<String, StoredDotNsName>>

    fun clear()
}
