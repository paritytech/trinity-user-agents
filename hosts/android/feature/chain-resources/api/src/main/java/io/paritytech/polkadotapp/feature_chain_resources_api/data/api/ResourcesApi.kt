package io.paritytech.polkadotapp.feature_chain_resources_api.data.api

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.resources
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chain_resources_api.data.model.OnChainConsumerInfo

@JvmInline
value class ResourcesApi(override val module: Module) : QueryableModule

val RuntimeMetadata.resources: ResourcesApi
    get() = ResourcesApi(resources())

@Deprecated("replace with DotNSGateway")
context(withRuntime: WithRuntime)
val ResourcesApi.consumers: QueryableStorageEntry1<AccountId, OnChainConsumerInfo>
    get() = storage1("Consumers")
