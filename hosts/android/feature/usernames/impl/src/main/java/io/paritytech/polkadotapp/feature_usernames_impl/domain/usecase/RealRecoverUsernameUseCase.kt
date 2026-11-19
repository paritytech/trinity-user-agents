package io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getWalletAccountIdIn
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalFullUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.RecoverUsernameUseCase
import javax.inject.Inject

class RealRecoverUsernameUseCase @Inject constructor(
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val localUsernameStorage: LocalUsernameStorage,
    private val localFullUsernameStorage: LocalFullUsernameStorage,
    private val resourcesRepository: ResourcesRepository,
    private val accountRepository: AccountRepository
) : RecoverUsernameUseCase {
    override suspend fun invoke(): Result<Boolean> {
        val chain = chainRegistry.getChain(knownChains.assetHub)
        val accountId = accountRepository.getWalletAccountIdIn(chain)

        return resourcesRepository.consumerInfo(chain.id, accountId)
            .map { consumerInfo ->
                val usernameFound = consumerInfo != null
                if (consumerInfo != null) {
                    localUsernameStorage.saveValue(Username.fromFullValue(consumerInfo.liteUsername))
                    consumerInfo.fullUsername?.let {
                        localFullUsernameStorage.saveValue(Username.fromFullValue(it))
                    }
                }
                usernameFound
            }
    }
}
