package io.paritytech.polkadotapp.feature_members_impl.data.network.blockchain.api

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry2
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry3
import io.paritytech.polkadotapp.chains.storage.source.query.api.constant
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage2
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage3
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.feature_members_api.data.model.PageIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollection
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingExponent
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingKeys
import io.paritytech.polkadotapp.feature_members_api.data.model.RingMembersState
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRoot
import io.paritytech.polkadotapp.feature_members_api.data.model.RingStatus

@JvmInline
value class MembersApi(override val module: Module) : QueryableModule

val RuntimeMetadata.members: MembersApi
    get() = MembersApi(module(Modules.MEMBERS))

context(withRuntime: WithRuntime)
val MembersApi.members: QueryableStorageEntry2<RingCollectionId, BandersnatchPublicKey, RingPosition>
    get() = storage2("Members")

context(withRuntime: WithRuntime)
val MembersApi.root: QueryableStorageEntry2<RingCollectionId, RingIndex, RingRoot>
    get() = storage2("Root")

context(withRuntime: WithRuntime)
val MembersApi.ringKeysStatus: QueryableStorageEntry2<RingCollectionId, RingIndex, RingStatus>
    get() = storage2("RingKeysStatus")

val MembersApi.ringKeys: QueryableStorageEntry3<RingCollectionId, RingIndex, PageIndex, RingKeys>
    get() = storage3("RingKeys")

context(withRuntime: WithRuntime)
val MembersApi.collections: QueryableStorageEntry1<RingCollectionId, RingCollection>
    get() = storage1("Collections")

context(withRuntime: WithRuntime)
val MembersApi.ringsState: QueryableStorageEntry1<RingCollectionId, RingMembersState>
    get() = storage1("RingsState")

/** Its capacity is the page size of [ringKeys]. */
context(withRuntime: WithRuntime)
val MembersApi.maxFlexibleRingExponent: RingExponent
    get() = constant("MaxFlexibleRingExponent")

context(withRuntime: WithRuntime)
val MembersApi.onboardingSize: QueryableStorageEntry1<RingCollectionId, Int>
    get() = storage1("OnboardingSize")
