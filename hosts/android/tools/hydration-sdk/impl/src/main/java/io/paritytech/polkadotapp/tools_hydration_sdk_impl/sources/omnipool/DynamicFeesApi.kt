@file:Suppress("RedundantUnitExpression")

package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.dynamicFees
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.DynamicFee
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model.bindDynamicFee

@JvmInline
value class DynamicFeesApi(override val module: Module) : QueryableModule

val RuntimeMetadata.dynamicFeesApi: DynamicFeesApi
    get() = DynamicFeesApi(dynamicFees())

context(storage: StorageQueryContext)
val DynamicFeesApi.assetFee: QueryableStorageEntry1<HydraDxAssetId, DynamicFee>
    get() = storage1(
        name = "AssetFee",
        binding = { decoded, _ -> bindDynamicFee(decoded) },
    )
