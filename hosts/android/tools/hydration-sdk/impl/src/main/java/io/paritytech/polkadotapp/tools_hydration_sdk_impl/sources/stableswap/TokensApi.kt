package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.tokens
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId

@JvmInline
value class TokensApi(override val module: Module) : QueryableModule

val RuntimeMetadata.hydraTokens: TokensApi
    get() = TokensApi(tokens())

context(storage: StorageQueryContext)
val TokensApi.totalIssuance: QueryableStorageEntry1<HydraDxAssetId, Balance>
    get() = storage1(
        name = "TotalIssuance",
        binding = { decoded, _ -> bindBalance(decoded) },
    )
