package io.paritytech.polkadotapp.feature_dotns_impl.data.blockchain

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.bindByteArray
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry0
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage0
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.networkSuffix

@JvmInline
value class NetworkSuffixApi(override val module: Module) : QueryableModule

val RuntimeMetadata.networkSuffix: NetworkSuffixApi
    get() = NetworkSuffixApi(networkSuffix())

context(withRuntime: WithRuntime)
val NetworkSuffixApi.networkSuffix: QueryableStorageEntry0<ByteArray>
    get() = storage0("NetworkSuffix", binding = ::bindByteArray)
