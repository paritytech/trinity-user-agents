package io.paritytech.polkadotapp.feature_pgas_impl.data.extension

import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersSubscriberRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.awaitRingRevision
import io.paritytech.polkadotapp.feature_members_api.data.repository.getRingRoot
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.toRingCollectionId

class AsPgas(
    private val period: UInt,
    private val context: BandersnatchContext,
    private val collection: PeopleCollection,
    private val peopleMembershipProver: PeopleMembershipProver,
    private val membersRepository: MembersRepository,
    private val membersSubscriberRepository: MembersSubscriberRepository,
    private val chainRegistry: ChainRegistry,
    private val chainStateRepository: ChainStateRepository,
) : TransactionExtension {
    override val name: String = "AsPgas"

    override suspend fun implicit(): Any? = null

    override suspend fun explicit(
        inheritedImplication: InheritedImplication,
        runtimeSnapshot: RuntimeSnapshot,
    ): Any? {
        val chainId = chainRegistry.knownChains.people
        val message = inheritedImplication.encoded().blake2b256()

        val blockHash = chainStateRepository.currentBlockHash(chainId)

        val proofResult = peopleMembershipProver.proofPersonMembership(
            message = message,
            context = context,
            chainId = chainId,
            peopleCollection = collection,
            at = blockHash
        ).getOrThrow()

        val revision = membersRepository.getRingRoot(
            chainId = chainId,
            collectionId = collection.toRingCollectionId(),
            ringIndex = proofResult.ringIndex,
            consistency = CacheableDataConsistency.CONSISTENT_WITH_REMOTE,
            blockHash = blockHash
        ).requireNotNull().getOrThrow().revision

        membersSubscriberRepository.awaitRingRevision(
            chainId = chainRegistry.knownChains.assetHub,
            collectionId = collection.toRingCollectionId(),
            ringIndex = proofResult.ringIndex,
            revision = revision,
        ).getOrThrow()

        return AsPgasInfoScale.Claim(
            proof = proofResult.proof,
            ringIndex = proofResult.ringIndex,
            revision = revision,
            collection = collection.toScale(),
            day = period,
        ).toEncodableInstance()
    }
}

private fun PeopleCollection.toScale(): PgasCollectionScale = when (this) {
    PeopleCollection.People -> PgasCollectionScale.People
    PeopleCollection.LitePeople -> PgasCollectionScale.LitePeople
}
