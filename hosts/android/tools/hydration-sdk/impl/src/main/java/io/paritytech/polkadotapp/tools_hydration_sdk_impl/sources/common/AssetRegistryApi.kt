package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.bindInt
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.assetRegistry
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId

@JvmInline
value class AssetRegistryApi(override val module: Module) : QueryableModule

val RuntimeMetadata.assetRegistry: AssetRegistryApi
    get() = AssetRegistryApi(assetRegistry())

context(storage: StorageQueryContext)
val AssetRegistryApi.assets: QueryableStorageEntry1<HydraDxAssetId, HydrationAssetMetadata>
    get() = storage1(name = "Assets", binding = ::bindHydrationAssetMetadata)

private fun bindHydrationAssetMetadata(
    decoded: Any,
    assetId: HydraDxAssetId
): HydrationAssetMetadata {
    val asStruct = decoded.castToStruct()

    return HydrationAssetMetadata(
        assetId = assetId,
        decimals = bindInt(asStruct["decimals"]),
        assetType = asStruct.get<DictEnum.Entry<*>>("assetType")!!.name
    )
}
