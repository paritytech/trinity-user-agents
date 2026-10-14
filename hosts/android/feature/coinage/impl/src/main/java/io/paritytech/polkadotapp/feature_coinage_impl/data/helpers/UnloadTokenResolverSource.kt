package io.paritytech.polkadotapp.feature_coinage_impl.data.helpers

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getAliasInContext
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.getFreeUnloadTokenInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.unloadTokenTimePeriodPeopleLitePeople
import javax.inject.Inject

interface UnloadTokenResolverSource {
    suspend fun getPeriodDuration(chainId: ChainId): Long

    suspend fun getFreeUnloadTokenLimit(chainId: ChainId): Result<Long>

    suspend fun generateAlias(context: ByteArray): ByteArray
}

class PeopleLiteUnloadTokenResolverSource @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val accountRepository: AccountRepository,
    private val bandersnatchStorage: BandersnatchSecretsStorage,
    private val viewFunctionsApi: MultiChainViewFunctionsApi,
) : UnloadTokenResolverSource {
    override suspend fun getPeriodDuration(chainId: ChainId): Long {
        return chainRegistry.withRuntime(chainId) {
            runtime.metadata.coinage.unloadTokenTimePeriodPeopleLitePeople
        }
    }

    override suspend fun getFreeUnloadTokenLimit(chainId: ChainId): Result<Long> {
        return viewFunctionsApi.forChain(chainId)
            .getFreeUnloadTokenInfo()
            .map { info -> info.litePeopleLimit }
    }

    override suspend fun generateAlias(context: ByteArray): ByteArray {
        val metaAccount = accountRepository.getWalletAccount()
        return bandersnatchStorage.getAliasInContext(
            metaAccount.id,
            BandersnatchContext(context)
        ).value
    }
}

class PeopleUnloadTokenResolverSource @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val bandersnatchStorage: BandersnatchSecretsStorage,
    private val accountRepository: AccountRepository,
    private val viewFunctionsApi: MultiChainViewFunctionsApi,
) : UnloadTokenResolverSource {
    override suspend fun getPeriodDuration(chainId: ChainId): Long {
        return chainRegistry.withRuntime(chainId) {
            runtime.metadata.coinage.unloadTokenTimePeriodPeopleLitePeople
        }
    }

    override suspend fun getFreeUnloadTokenLimit(chainId: ChainId): Result<Long> {
        return viewFunctionsApi.forChain(chainId)
            .getFreeUnloadTokenInfo()
            .map { info -> info.peopleLimit }
    }

    override suspend fun generateAlias(context: ByteArray): ByteArray {
        val metaAccount = accountRepository.getCandidateAccount()
        return bandersnatchStorage.getAliasInContext(
            metaAccount.id,
            BandersnatchContext(context)
        ).value
    }
}
