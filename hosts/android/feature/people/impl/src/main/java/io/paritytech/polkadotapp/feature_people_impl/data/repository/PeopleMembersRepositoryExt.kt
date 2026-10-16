package io.paritytech.polkadotapp.feature_people_impl.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.feature_account_api.domain.model.PersonPublicKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.subscribeMember
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE
import kotlinx.coroutines.flow.Flow

fun MembersRepository.subscribePersonMember(
    chainId: ChainId,
    key: PersonPublicKey,
    consistency: CacheableDataConsistency,
): Flow<Result<RingPosition?>> {
    return subscribeMember(
        chainId = chainId,
        collectionId = RingCollectionId.PEOPLE,
        key = key,
        consistency = consistency,
    )
}
