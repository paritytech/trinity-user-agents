package io.paritytech.polkadotapp.feature_people_impl.domain.useCase

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMemberKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_people_api.domain.models.PersonhoodStatus
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.PersonStatusUseCase
import io.paritytech.polkadotapp.feature_people_impl.data.repository.subscribePersonMember
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealPersonStatusUseCase @Inject constructor(
    private val membersRepository: MembersRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val accountRepository: AccountRepository,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
) : PersonStatusUseCase {
    override fun personhoodStatusFlow(): Flow<PersonhoodStatus> = flowOfAll {
        val chain = peopleChain()

        getCandidateMemberRecord(chain).map {
            when (it) {
                null -> PersonhoodStatus.NotPerson
                is RingPosition.Suspended -> PersonhoodStatus.Suspended
                is RingPosition.Onboarding -> PersonhoodStatus.Onboarding
                is RingPosition.Included -> PersonhoodStatus.Active
            }
        }
    }

    private fun getCandidateMemberRecord(chain: Chain): Flow<RingPosition?> {
        return flowOfAll {
            val account = accountRepository.getCandidateAccount()
            val personKey = bandersnatchSecretsStorage.getMemberKey(account.id)

            membersRepository.subscribePersonMember(
                chainId = chain.id,
                key = personKey,
                consistency = CacheableDataConsistency.CAN_BE_STALE,
            ).map { it.getOrNull() }
        }
    }

    private suspend fun peopleChain(): Chain {
        return chainRegistry.getChain(knownChains.people)
    }
}
