package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs.DotNsBuild

interface DotNsBuildStorage {
    fun get(dotNsName: String): DotNsBuild?

    fun putAll(builds: Map<String, DotNsBuild>)

    fun all(): Map<String, DotNsBuild>

    fun clear()
}
