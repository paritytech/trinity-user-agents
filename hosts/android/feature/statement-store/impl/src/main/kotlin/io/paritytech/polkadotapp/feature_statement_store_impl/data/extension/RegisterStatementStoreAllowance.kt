package io.paritytech.polkadotapp.feature_statement_store_impl.data.extension

import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.getChainIdByGenesisHashOrThrow
import io.paritytech.polkadotapp.chains.util.findGenesisHashOrThrow
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver

class RegisterStatementStoreAllowance(
    private val context: BandersnatchContext,
    private val collection: PeopleCollection,
    private val peopleMembershipProver: PeopleMembershipProver,
    private val chainRegistry: ChainRegistry,
) : TransactionExtension {
    override val name: String = "AsResources"

    override suspend fun implicit(): Any? = null

    override suspend fun explicit(
        inheritedImplication: InheritedImplication,
        runtimeSnapshot: RuntimeSnapshot,
    ): Any? {
        val chainId = chainRegistry.getChainIdByGenesisHashOrThrow(inheritedImplication.findGenesisHashOrThrow())
        val message = inheritedImplication.encoded().blake2b256()

        val proofResult = peopleMembershipProver.proofPersonMembership(
            message = message,
            context = context,
            chainId = chainId,
            peopleCollection = collection,
        ).getOrThrow()

        return AsResourcesInfoScale.RegisterStatementStoreAllowance(
            proof = proofResult.proof,
            ringIndex = proofResult.ringIndex,
            revision = proofResult.revision,
            collection = collection.toScale(),
        ).toEncodableInstance()
    }
}

private fun PeopleCollection.toScale(): MembershipCollectionScale = when (this) {
    PeopleCollection.People -> MembershipCollectionScale.People
    PeopleCollection.LitePeople -> MembershipCollectionScale.LitePeople
}
