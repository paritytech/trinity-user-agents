package io.paritytech.polkadotapp.chains.storage.typed

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry0
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage0
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import java.math.BigInteger

@JvmInline
value class TimestampRuntimeApi(override val module: Module) : QueryableModule

val RuntimeMetadata.timestamp: TimestampRuntimeApi
    get() = TimestampRuntimeApi(module(Modules.TIMESTAMP))

context(withRuntime: WithRuntime)
val TimestampRuntimeApi.now: QueryableStorageEntry0<BigInteger>
    get() = storage0("Now", binding = ::bindNumber)
