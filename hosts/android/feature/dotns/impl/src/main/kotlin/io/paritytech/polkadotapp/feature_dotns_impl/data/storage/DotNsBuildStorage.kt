package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import kotlinx.coroutines.flow.Flow

/** The builds of a name on the device: [served] answers its requests, [update] is a newer one downloaded and not yet applied. */
data class DotNsBuild(val served: ContentHash, val update: ContentHash?)

/** Which build each name serves, kept across app starts so a name on the device is served without chain. */
interface DotNsBuildStorage {
    fun get(dotNsName: String): DotNsBuild?

    fun put(dotNsName: String, build: DotNsBuild)

    fun all(): Map<String, DotNsBuild>

    fun observeAll(): Flow<Map<String, DotNsBuild>>

    fun clear()
}
