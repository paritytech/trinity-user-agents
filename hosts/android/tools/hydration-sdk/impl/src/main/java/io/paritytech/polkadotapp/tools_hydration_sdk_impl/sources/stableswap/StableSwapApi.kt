package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.stableSwap
import io.paritytech.polkadotapp.chains.util.stableSwapOrNull
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.StableSwapPoolInfo
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.StalbeSwapPoolPegInfo
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.bindPoolPegInfo
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model.bindStablePoolInfo

@JvmInline
value class StableSwapApi(override val module: Module) : QueryableModule

val RuntimeMetadata.stableSwapOrNull: StableSwapApi?
    get() = stableSwapOrNull()?.let(::StableSwapApi)

val RuntimeMetadata.stableSwap: StableSwapApi
    get() = StableSwapApi(stableSwap())

context(storage: StorageQueryContext)
val StableSwapApi.pools: QueryableStorageEntry1<HydraDxAssetId, StableSwapPoolInfo>
    get() = storage1(
        name = "Pools",
        binding = ::bindStablePoolInfo,
    )

context(storage: StorageQueryContext)
val StableSwapApi.poolPegs: QueryableStorageEntry1<HydraDxAssetId, StalbeSwapPoolPegInfo>
    get() = storage1(
        name = "PoolPegs",
        binding = { decoded, _ -> bindPoolPegInfo(decoded) },
    )
