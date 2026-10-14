package io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.api

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.UntypedAssetsAssetId
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry2
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage2
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model.AssetsAccount
import io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model.AssetsAssetDetails

@JvmInline
internal value class AssetsApi(override val module: Module) : QueryableModule

internal fun RuntimeMetadata.assets(palletName: String): AssetsApi {
    return AssetsApi(module(palletName))
}

context(withRuntime: WithRuntime)
internal val AssetsApi.asset: QueryableStorageEntry1<UntypedAssetsAssetId, AssetsAssetDetails>
    get() = storage1("Asset")

context(withRuntime: WithRuntime)
internal val AssetsApi.account: QueryableStorageEntry2<UntypedAssetsAssetId, AccountId, AssetsAccount>
    get() = storage2("Account")
