package io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry0
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry3
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage0
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage3
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCommitmentRecord
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import java.math.BigInteger

@JvmInline
value class MembersSubscriberApi(override val module: Module) : QueryableModule

val RuntimeMetadata.membersSubscriber: MembersSubscriberApi
    get() = MembersSubscriberApi(module(Modules.MEMBERS_SUBSCRIBER))

context(withRuntime: WithRuntime)
val MembersSubscriberApi.currentGeneration: QueryableStorageEntry0<UInt>
    get() = storage0("CurrentGeneration")

context(withRuntime: WithRuntime)
val MembersSubscriberApi.ringRoots: QueryableStorageEntry3<BigInteger, RingCollectionId, RingIndex, List<RingCommitmentRecord>>
    get() = storage3("RingRoots")
