@file:Suppress("RedundantUnitExpression")

package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.omnipool
import io.paritytech.polkadotapp.chains.util.omnipoolOrNull
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.OmnipoolAssetState
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.bindOmnipoolAssetState

@JvmInline
value class OmnipoolApi(override val module: Module) : QueryableModule

val RuntimeMetadata.omnipoolOrNull: OmnipoolApi?
    get() = omnipoolOrNull()?.let(::OmnipoolApi)

val RuntimeMetadata.omnipool: OmnipoolApi
    get() = OmnipoolApi(omnipool())

context(storage: StorageQueryContext)
val OmnipoolApi.assets: QueryableStorageEntry1<HydraDxAssetId, OmnipoolAssetState>
    get() = storage1(
        name = "Assets",
        binding = ::bindOmnipoolAssetState,
    )
