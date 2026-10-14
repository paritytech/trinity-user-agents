@file:Suppress("RedundantUnitExpression")

package io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.api

import io.novasama.substrate_sdk_android.runtime.definitions.types.RuntimeType
import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.method
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.novasama.substrate_sdk_android.runtime.metadata.runtimeApiOrNull
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.assetConversionOrNull
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation

@JvmInline
internal value class AssetConversionApi(override val module: Module) : QueryableModule

internal val RuntimeMetadata.assetConversionOrNull: AssetConversionApi?
    get() = assetConversionOrNull()?.let(::AssetConversionApi)

context(storage: StorageQueryContext)
internal val AssetConversionApi.pools: QueryableStorageEntry1<Tuple2<RelativeMultiLocation, RelativeMultiLocation>, Unit>
    get() = storage1(name = "Pools")

fun RuntimeMetadata.assetConversionAssetIdType(): RuntimeType<*, *>? {
    val runtimeApi = runtimeApiOrNull("AssetConversionApi") ?: return null

    return runtimeApi.method("quote_price_tokens_for_exact_tokens")
        .inputs.first().type
}
