package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry2
import io.paritytech.polkadotapp.chains.storage.source.query.api.constant
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage2
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.resources
import io.paritytech.polkadotapp.common.utils.scale.BigEndianU32Scale

@JvmInline
value class LtsResourcesApi(override val module: Module) : QueryableModule

val RuntimeMetadata.ltsResources: LtsResourcesApi
    get() = LtsResourcesApi(resources())

context(withRuntime: WithRuntime)
val LtsResourcesApi.longTermStoragePeriodDuration: UInt
    get() = constant("LongTermStoragePeriodDuration")

context(withRuntime: WithRuntime)
val LtsResourcesApi.spentLongTermStorageAliases: QueryableStorageEntry2<BigEndianU32Scale, BandersnatchAlias, Unit>
    get() = storage2("SpentLongTermStorageAliases")
