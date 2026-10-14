package io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolver
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.extensions.AsCoinageInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.extensions.AsCoinageTxExtensionFactory
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PrecomputedPersonMembershipProver
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SetTransactionExtensionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class CoinageTransactionOrigins @Inject constructor(
    private val keypairDerivation: CoinKeypairDerivation,
    private val asCoinageTxExtensionFactory: AsCoinageTxExtensionFactory,
) {
    suspend fun createAsCoinOrigin(coin: Coin): TransactionOrigin {
        return createAsCoinOrigin(keypairDerivation.deriveKeypair(coin.derivationIndex))
    }

    fun createAsCoinOrigin(keypair: Keypair): TransactionOrigin {
        val signerSource = TransactionSignerSource.FromKeyPair(
            keypair = keypair,
            encryption = MultiChainEncryption.Substrate(EncryptionType.SR25519)
        )
        return SetTransactionExtensionOrigin(
            signerSource = signerSource,
            transactionExtension = asCoinageTxExtensionFactory.create(AsCoinageInfo.AsCoin),
        )
    }

    fun createAsUnloadTokenPeopleOrigin(
        recyclerRevisionBlockHash: BlockHash,
        vouchers: List<RecyclerVoucher>,
        resolvedUnloadToken: FreeUnloadTokenResolver.ResolvedUnloadToken,
        personProver: PrecomputedPersonMembershipProver,
        peopleCollection: PeopleCollection,
    ): TransactionOrigin {
        val info = AsCoinageInfo.AsFreeUnloadToken(
            vouchers = vouchers,
            resolvedToken = resolvedUnloadToken,
            recyclerRevisionBlockHash = recyclerRevisionBlockHash,
            personProver = personProver,
            peopleCollection = peopleCollection,
        )
        return SetTransactionExtensionOrigin(
            signerSource = TransactionSignerSource.None,
            transactionExtension = asCoinageTxExtensionFactory.create(info),
        )
    }

    // Used for load_recycler_with_external_asset_unpaid, must be signed
    fun createInfallibleUnpaidSigned(
        signerSource: TransactionSignerSource.Signed
    ): TransactionOrigin {
        return SetTransactionExtensionOrigin(
            signerSource = signerSource,
            transactionExtension = asCoinageTxExtensionFactory.create(AsCoinageInfo.InfallibleUnpaidSigned),
        )
    }
}
