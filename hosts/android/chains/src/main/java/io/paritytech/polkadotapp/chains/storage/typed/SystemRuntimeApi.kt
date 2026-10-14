package io.paritytech.polkadotapp.chains.storage.typed

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.AccountInfo
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.binding.EventRecord
import io.paritytech.polkadotapp.chains.network.binding.bindBlockNumber
import io.paritytech.polkadotapp.chains.network.binding.bindEventRecords
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry0
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage0
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.domain.model.AccountId

@JvmInline
value class SystemRuntimeApi(override val module: Module) : QueryableModule

val RuntimeMetadata.system: SystemRuntimeApi
    get() = SystemRuntimeApi(module(Modules.SYSTEM))

context(withRuntime: WithRuntime)
val SystemRuntimeApi.number: QueryableStorageEntry0<BlockNumber>
    get() = storage0("Number", binding = ::bindBlockNumber)

context(withRuntime: WithRuntime)
val SystemRuntimeApi.account: QueryableStorageEntry1<AccountId, AccountInfo>
    get() = storage1("Account")

context(withRuntime: WithRuntime)
val SystemRuntimeApi.events: QueryableStorageEntry0<List<EventRecord>>
    get() = storage0("Events", binding = ::bindEventRecords)
