package io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.extensions

import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getChainIdByGenesisHashOrThrow
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.util.findGenesisHashOrThrow
import io.paritytech.polkadotapp.chains.util.findNonceOrThrow
import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.toRingCollectionId
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.deriveBandersnatchForVouchers
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.common.getCommonRecyclerKey
import io.paritytech.polkadotapp.feature_members_api.domain.MembershipProver
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProof
import javax.inject.Inject

class AsCoinageTxExtensionFactory @Inject constructor(
    private val coinageSigningContextProvider: CoinageSigningContextProvider,
    private val membershipProver: MembershipProver,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val chainRegistry: ChainRegistry,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) {
    fun create(info: AsCoinageInfo): AsCoinageTxExtension {
        return AsCoinageTxExtension(
            info = info,
            coinageSigningContextProvider = coinageSigningContextProvider,
            membershipProver = membershipProver,
            voucherRingDerivation = voucherRingDerivation,
            chainRegistry = chainRegistry,
            coinageInstanceIdProvider = coinageInstanceIdProvider,
        )
    }
}

class AsCoinageTxExtension(
    private val info: AsCoinageInfo,
    private val coinageSigningContextProvider: CoinageSigningContextProvider,
    private val membershipProver: MembershipProver,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val chainRegistry: ChainRegistry,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) : TransactionExtension {
    override val name: String = "AsCoinage"

    override suspend fun implicit(): Any? = null

    override suspend fun explicit(
        inheritedImplication: InheritedImplication,
        runtimeSnapshot: RuntimeSnapshot
    ): Any? {
        val payload = when (info) {
            is AsCoinageInfo.AsCoin -> AsCoinageInfoScale.AsCoin
            is AsCoinageInfo.AsFreeUnloadToken -> asFreeUnloadToken(info, inheritedImplication)
            AsCoinageInfo.InfallibleUnpaidSigned -> infallibleUnpaidSigned(inheritedImplication)
        }

        return payload.toEncodableInstance()
    }

    private fun infallibleUnpaidSigned(inheritedImplication: InheritedImplication): AsCoinageInfoScale.InfallibleUnpaidSigned {
        val nonce = inheritedImplication.findNonceOrThrow()
        return AsCoinageInfoScale.InfallibleUnpaidSigned(nonce)
    }

    private suspend fun asFreeUnloadToken(
        info: AsCoinageInfo.AsFreeUnloadToken,
        inheritedImplication: InheritedImplication
    ): AsCoinageInfoScale {
        val chainId = chainRegistry.getChainIdByGenesisHashOrThrow(inheritedImplication.findGenesisHashOrThrow())

        val encodedImplication = inheritedImplication.encoded()

        val voucherProofMessage = encodedImplication.blake2b256()
        val aliasProofs = makeRecyclerAliasProofs(
            chainId = chainId,
            message = voucherProofMessage,
            vouchers = info.vouchers,
            recyclerRevisionBlockHash = info.recyclerRevisionBlockHash,
        )

        val proofContext = coinageSigningContextProvider.freeUnloadTokenContext(
            info.resolvedToken.period.toInt(),
            info.resolvedToken.counter.toInt()
        )

        val personProofMessage = createPersonProofMessage(encodedImplication, aliasProofs)
        val personProof = info.personProver.proofPersonMembership(personProofMessage, proofContext).getOrThrow()

        val body = AsCoinageInfoScale.Body(
            proof = personProof.toPeopleRingProof(),
            period = info.resolvedToken.period,
            counter = info.resolvedToken.counter,
            aliasProofs = aliasProofs,
        )

        return when (info.peopleCollection) {
            PeopleCollection.People -> AsCoinageInfoScale.AsUnloadTokenPeople(body)
            PeopleCollection.LitePeople -> AsCoinageInfoScale.AsUnloadTokenLitePeople(body)
        }
    }

    private fun createPersonProofMessage(
        encodedIInheritedImplication: ByteArray,
        aliasProofs: List<BandersnatchProof>
    ): ByteArray {
        return (aliasProofs.scaleEncodeBinary() + encodedIInheritedImplication).blake2b256()
    }

    private suspend fun makeRecyclerAliasProofs(
        chainId: ChainId,
        message: ByteArray,
        vouchers: List<RecyclerVoucher>,
        recyclerRevisionBlockHash: BlockHash,
    ): List<BandersnatchProof> {
        val recyclerKey = vouchers.getCommonRecyclerKey()
        val voucherEntropies = voucherRingDerivation.deriveBandersnatchForVouchers(vouchers)

        return membershipProver.proofMembershipBatched(
            members = voucherEntropies.map(MemberSource::Entropy),
            message = message,
            context = coinageSigningContextProvider.recyclerVouchersContext(),
            chainId = chainId,
            collectionId = recyclerKey.exponent.toRingCollectionId(coinageInstanceIdProvider.instanceId().getOrThrow()),
            ringIndex = recyclerKey.recyclerIndex,
            blockHash = recyclerRevisionBlockHash,
        ).getOrThrow()
    }

    private fun PeopleMembershipProof.toPeopleRingProof(): PeopleRingProof {
        return PeopleRingProof(proof, ringIndex, revision)
    }
}
