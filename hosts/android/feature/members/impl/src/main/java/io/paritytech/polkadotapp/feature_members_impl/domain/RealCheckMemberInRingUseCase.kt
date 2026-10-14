package io.paritytech.polkadotapp.feature_members_impl.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.bandersnatch_crypto.memberKey
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.utils.combineResults
import io.paritytech.polkadotapp.common.utils.firstIsInstance
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatMapNotNull
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMemberKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.model.includedOrNull
import io.paritytech.polkadotapp.feature_members_api.data.model.includesKey
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.getMember
import io.paritytech.polkadotapp.feature_members_api.data.repository.getRingStatus
import io.paritytech.polkadotapp.feature_members_api.data.repository.subscribeMember
import io.paritytech.polkadotapp.feature_members_api.data.repository.subscribeRingStatus
import io.paritytech.polkadotapp.feature_members_api.domain.CheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import javax.inject.Inject

class RealCheckMemberInRingUseCase @Inject constructor(
    private val membersRepository: MembersRepository,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
) : CheckMemberInRingUseCase {
    override suspend fun awaitIncluded(
        chainId: ChainId,
        collectionId: RingCollectionId,
        memberSource: MemberSource,
    ): Result<Unit> {
        return combineResults(resolveKey(memberSource), membersRepository.getRingKeysPageSize(chainId)) { key, keysPerPage ->
            key to keysPerPage
        }.mapCatching { (key, keysPerPage) ->
            val included = awaitMemberIncluded(chainId, collectionId, key)
            awaitRingIncludesKey(chainId, collectionId, included, keysPerPage)
        }
    }

    override suspend fun checkIncludes(
        chainId: ChainId,
        collectionId: RingCollectionId,
        memberSource: MemberSource,
    ): Result<Boolean> {
        return resolveKey(memberSource).flatMap { key ->
            membersRepository.getMember(chainId, collectionId, key, FRESH)
                .map { it?.includedOrNull() }
                .flatMapNotNull { included ->
                    combineResults(
                        membersRepository.getRingStatus(chainId, collectionId, included.ringIndex, FRESH),
                        membersRepository.getRingKeysPageSize(chainId),
                    ) { status, keysPerPage -> status != null && status.includesKey(included, keysPerPage) }
                }
                .map { it ?: false }
        }
    }

    private suspend fun awaitMemberIncluded(
        chainId: ChainId,
        collectionId: RingCollectionId,
        key: BandersnatchPublicKey,
    ): RingPosition.Included {
        return membersRepository.subscribeMember(chainId, collectionId, key, FRESH)
            .mapNotNull { it.getOrNull() }
            .firstIsInstance()
    }

    private suspend fun awaitRingIncludesKey(
        chainId: ChainId,
        collectionId: RingCollectionId,
        included: RingPosition.Included,
        keysPerPage: Int,
    ) {
        membersRepository.subscribeRingStatus(chainId, collectionId, included.ringIndex)
            .mapNotNull { it.getOrNull() }
            .first { it.includesKey(included, keysPerPage) }
    }

    private suspend fun resolveKey(memberSource: MemberSource): Result<BandersnatchPublicKey> = runCatching {
        when (memberSource) {
            is MemberSource.Account -> bandersnatchSecretsStorage.getMemberKey(memberSource.metaId)
            is MemberSource.Entropy -> memberSource.bandersnatchEntropy.memberKey()
        }
    }

    companion object {
        private val FRESH = CacheableDataConsistency.CONSISTENT_WITH_REMOTE
    }
}
