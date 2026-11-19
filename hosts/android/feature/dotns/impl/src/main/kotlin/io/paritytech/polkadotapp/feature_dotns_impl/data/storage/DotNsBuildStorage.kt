package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs.DotNsNameVersion

interface DotNsBuildStorage {
    fun get(dotNsName: String): DotNsNameVersion?

    fun put(dotNsName: String, served: DotNsNameVersion)

    fun clear()
}
