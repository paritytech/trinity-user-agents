package io.paritytech.polkadotapp.feature_dotns_gateway_api.data.api

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.dotNsGateway
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.model.DotNsOnChainConsumerInfo

@JvmInline
value class DotNsGatewayApi(override val module: Module) : QueryableModule

val RuntimeMetadata.dotNsGateway: DotNsGatewayApi
    get() = DotNsGatewayApi(dotNsGateway())

context(withRuntime: WithRuntime)
val DotNsGatewayApi.liteLabelOwner: QueryableStorageEntry1<String, AccountId>
    get() = storage1("LiteLabelOwner")

context(withRuntime: WithRuntime)
val DotNsGatewayApi.accountAlias: QueryableStorageEntry1<AccountId, DataByteArray>
    get() = storage1("AccountAlias")

context(withRuntime: WithRuntime)
val DotNsGatewayApi.accountNames: QueryableStorageEntry1<AccountId, DotNsOnChainConsumerInfo>
    get() = storage1("AccountNames")
