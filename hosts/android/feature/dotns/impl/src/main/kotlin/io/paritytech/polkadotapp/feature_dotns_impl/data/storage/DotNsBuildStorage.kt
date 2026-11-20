package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs.DotNsBuild
import kotlinx.coroutines.flow.Flow

interface DotNsBuildStorage {
    fun get(dotNsName: String): DotNsBuild?

    fun putAll(builds: Map<String, DotNsBuild>)

    fun all(): Map<String, DotNsBuild>

    fun observeAll(): Flow<Map<String, DotNsBuild>>

    fun clear()
}
