package io.paritytech.polkadotapp.feature_revive_impl.data

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId

@JvmInline
value class ReviveRuntimeApi(override val module: Module) : QueryableModule

val RuntimeMetadata.revive: ReviveRuntimeApi
    get() = ReviveRuntimeApi(module(Modules.REVIVE))

context(withRuntime: WithRuntime)
val ReviveRuntimeApi.originalAccount: QueryableStorageEntry1<EvmAccountId, AccountId>
    get() = storage1("OriginalAccount")
