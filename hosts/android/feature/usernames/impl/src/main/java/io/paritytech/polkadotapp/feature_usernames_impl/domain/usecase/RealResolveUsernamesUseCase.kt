package io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ResolveUsernamesUseCase
import javax.inject.Inject

class RealResolveUsernamesUseCase @Inject constructor(
    private val knownChains: KnownChains,
    private val resourcesRepository: ResourcesRepository
) : ResolveUsernamesUseCase {
    override suspend fun invoke(accountIds: Collection<AccountId>): Result<Map<AccountId, Username?>> {
        return resourcesRepository.resolveConsumers(knownChains.assetHub, accountIds)
            .map { allFound ->
                val notFoundKeys = accountIds - allFound.keys
                val notFoundEntries = notFoundKeys.associateWith { null }

                val result = allFound + notFoundEntries

                result.mapValues { (_, consumerInfo) ->
                    consumerInfo?.username?.let(Username::fromFullValue)
                }
            }
    }
}
