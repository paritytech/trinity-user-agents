package io.paritytech.polkadotapp.feature_pgas_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimSpec

interface PgasRepository : PgasClaimSpec {
    suspend fun maxClaimsPerPeriod(chainId: ChainId, collection: PeopleCollection): UInt

    suspend fun claimedAliases(
        chainId: ChainId,
        period: UInt,
        candidates: List<BandersnatchAlias>,
    ): Set<BandersnatchAlias>
}
