package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.xyk
import io.paritytech.polkadotapp.chains.util.xykOrNull
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model.XYKPoolInfo

@JvmInline
value class XYKSwapApi(override val module: Module) : QueryableModule

val RuntimeMetadata.xykOrNull: XYKSwapApi?
    get() = xykOrNull()?.let(::XYKSwapApi)

val RuntimeMetadata.xyk: XYKSwapApi
    get() = XYKSwapApi(xyk())

context(storage: StorageQueryContext)
val XYKSwapApi.poolAssets: QueryableStorageEntry1<AccountId, XYKPoolInfo>
    get() = storage1("PoolAssets")
