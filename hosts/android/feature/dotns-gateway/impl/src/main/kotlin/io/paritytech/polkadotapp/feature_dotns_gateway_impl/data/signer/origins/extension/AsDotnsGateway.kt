package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.extension

import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.data.substrate.model.MultiSignature
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.sign.AccountBytesSigner
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.DOTNS_GATEWAY
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersSubscriberRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.awaitRingRevision
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.toRingCollectionId

class AsDotnsGateway(
    private val who: AccountId,
    private val label: String,
    private val link: DotNsLink,
    private val peopleMembershipProver: PeopleMembershipProver,
    private val membersSubscriberRepository: MembersSubscriberRepository,
    private val accountBytesSigner: AccountBytesSigner,
    private val chainRegistry: ChainRegistry
) : TransactionExtension {
    override val name: String = "AsDotnsGateway"

    override suspend fun implicit(): Any? = null

    override suspend fun explicit(
        inheritedImplication: InheritedImplication,
        runtimeSnapshot: RuntimeSnapshot
    ): Any? {
        val collection = PeopleCollection.People
        val proofMessage = DotNsRegisterProofMessage.hash(who, label, link)

        val proofResult = peopleMembershipProver.proofPersonMembership(
            message = proofMessage,
            context = BandersnatchContext.DOTNS_GATEWAY,
            chainId = chainRegistry.knownChains.people,
            peopleCollection = collection
        ).getOrThrow()

        membersSubscriberRepository.awaitRingRevision(
            chainId = chainRegistry.knownChains.assetHub,
            collectionId = collection.toRingCollectionId(),
            ringIndex = proofResult.ringIndex,
            revision = proofResult.revision
        ).getOrThrow()

        val consentMessage = inheritedImplication.encoded().blake2b256()
        val consentSignature = accountBytesSigner.signRawBytesByWallet(
            message = consentMessage,
            chainId = chainRegistry.knownChains.assetHub,
            context = MessageSigningContext.trustedContent()
        ).getOrThrow()

        return AsDotnsGatewayInfoScale.RegisterFullName(
            proof = proofResult.proof,
            ringIndex = proofResult.ringIndex,
            revision = proofResult.revision,
            signature = MultiSignature.Sr25519(consentSignature.signature.toDataByteArray())
        ).toEncodableInstance()
    }
}
