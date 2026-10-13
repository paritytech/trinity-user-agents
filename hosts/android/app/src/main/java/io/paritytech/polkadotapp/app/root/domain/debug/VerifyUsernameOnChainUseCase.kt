package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getWalletAccountIdIn
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import javax.inject.Inject

class VerifyUsernameOnChainUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val resourcesRepository: ResourcesRepository,
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
) {
    suspend operator fun invoke(): Result<Boolean> = runCatching {
        val chain = chainRegistry.getChain(knownChains.assetHub)
        val accountId = accountRepository.getWalletAccountIdIn(chain)
        chain to accountId
    }.flatMap { (chain, accountId) ->
        resourcesRepository.consumerInfo(chain.id, accountId)
            .map { it != null }
    }
}
