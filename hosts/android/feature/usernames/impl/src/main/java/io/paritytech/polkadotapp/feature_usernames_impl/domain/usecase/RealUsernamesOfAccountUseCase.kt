package io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.model.ConsumerInfo
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.repository.DotNsGatewayRepository
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalFullUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.StoredUsername
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.transformLatest
import javax.inject.Inject

class RealUsernamesOfAccountUseCase @Inject constructor(
    private val localUsernameStorage: LocalUsernameStorage,
    private val localFullUsernameStorage: LocalFullUsernameStorage,
    private val resourcesRepository: ResourcesRepository,
    private val dotNsGatewayRepository: DotNsGatewayRepository,
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains
) : UsernameOfAccountUseCase {
    override operator fun invoke(): Flow<StoredUsername?> {
        return accountRepository.areAccountsInitializedFlow()
            .flatMapLatest { accountsInitialized ->
                if (accountsInitialized) {
                    onChain().transformLatest {
                        if (it != null) {
                            emit(it)
                        } else {
                            emitAll(local())
                        }
                    }.catch { emitAll(local()) }
                } else {
                    flowOf { null }
                }
            }
    }

    override fun initiallyClaimedLightUsername(): Flow<Username?> {
        return localUsernameStorage.valueFlow()
    }

    override suspend fun getUsername(): Result<StoredUsername?> {
        return runCatching {
            onChain().first() ?: getLocallyStoredUsername()
        }
    }

    private fun local(): Flow<StoredUsername?> = localUsernameStorage.valueFlow()
        .map { it?.toStoredLightUsername() }

    private suspend fun getLocallyStoredUsername(): StoredUsername? {
        return localUsernameStorage.getValue()?.toStoredLightUsername()
    }

    private fun onChain(): Flow<StoredUsername?> = accountRepository.walletAccountFlow()
        .flatMapLatest { account ->
            val accountId = account.accountIdIn(chainRegistry.getChain(knownChains.assetHub))

            combine(
                resourcesRepository.consumerInfoLocalFlow(knownChains.assetHub, accountId),
                dotNsGatewayRepository.observeHasFullUsername(accountId).onStart<Boolean?> { emit(null) },
                localFullUsernameStorage.valueFlow()
            ) { consumerInfo, hasFullAlias, localFullUsername ->
                consumerInfo?.toStoredUsername(hasFullAlias, localFullUsername)
            }
        }

    private fun ConsumerInfo.toStoredUsername(hasFullAlias: Boolean?, localFullUsername: Username?): StoredUsername {
        val chainFullUsername = fullUsername?.let(Username::fromFullValue)

        return StoredUsername(
            liteUsername = Username.fromFullValue(liteUsername),
            fullUsername = chainFullUsername ?: localFullUsername.takeIf { hasFullAlias != false },
            isOnChain = true
        )
    }

    private fun Username.toStoredLightUsername(): StoredUsername {
        return StoredUsername(
            liteUsername = this,
            fullUsername = null,
            isOnChain = false
        )
    }
}
