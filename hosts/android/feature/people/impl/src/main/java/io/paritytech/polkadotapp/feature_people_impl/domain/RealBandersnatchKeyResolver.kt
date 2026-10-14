package io.paritytech.polkadotapp.feature_people_impl.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getAliasInContext
import io.paritytech.polkadotapp.feature_people_api.domain.BandersnatchKeyResolver
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import javax.inject.Inject

class RealBandersnatchKeyResolver @Inject constructor(
    private val accountRepository: AccountRepository,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
) : BandersnatchKeyResolver {
    override suspend fun getAliasInContext(collection: PeopleCollection, context: BandersnatchContext): BandersnatchAlias {
        val metaId = when (collection) {
            PeopleCollection.People -> accountRepository.getCandidateAccount().id
            PeopleCollection.LitePeople -> accountRepository.getWalletAccount().id
        }
        return bandersnatchSecretsStorage.getAliasInContext(metaId, context)
    }
}
