package io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.parameters
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.model.RuntimeParametersKey
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.model.RuntimeParametersValue

@JvmInline
value class RuntimeParametersApi(override val module: Module) : QueryableModule

val RuntimeMetadata.runtimeParameters: RuntimeParametersApi
    get() = RuntimeParametersApi(parameters())

context(withRuntime: WithRuntime)
val RuntimeParametersApi.parameters: QueryableStorageEntry1<RuntimeParametersKey, RuntimeParametersValue>
    get() = storage1("Parameters")
