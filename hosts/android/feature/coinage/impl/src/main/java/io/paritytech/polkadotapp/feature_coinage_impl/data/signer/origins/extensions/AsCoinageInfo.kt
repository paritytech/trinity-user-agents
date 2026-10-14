package io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.extensions

import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolver
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PrecomputedPersonMembershipProver

sealed class AsCoinageInfo {
    object AsCoin : AsCoinageInfo()

    class AsFreeUnloadToken(
        val vouchers: List<RecyclerVoucher>,
        val resolvedToken: FreeUnloadTokenResolver.ResolvedUnloadToken,
        val recyclerRevisionBlockHash: BlockHash,
        val personProver: PrecomputedPersonMembershipProver,
        val peopleCollection: PeopleCollection,
    ) : AsCoinageInfo()

    object InfallibleUnpaidSigned : AsCoinageInfo()
}
