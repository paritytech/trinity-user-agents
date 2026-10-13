package io.paritytech.polkadotapp.feature_chain_resources_impl.data.repository

import io.paritytech.polkadotapp.chains.di.LocalSourceQualifier
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.scale.toDomain
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.consumers
import io.paritytech.polkadotapp.feature_chain_resources_api.data.api.resources
import io.paritytech.polkadotapp.feature_chain_resources_api.data.model.OnChainConsumerInfo
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.model.ConsumerInfo
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealResourcesRepository @Inject constructor(
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    @LocalSourceQualifier private val localStorageSource: StorageDataSource,
) : ResourcesRepository {
    override suspend fun consumerInfo(
        chainId: ChainId,
        accountId: AccountId,
    ): Result<ConsumerInfo?> {
        return remoteStorageSource.queryCatching(chainId) {
            val onChainInfo = metadata.resources.consumers.query(accountId)
            onChainInfo?.toDomain(accountId)
        }
    }

    override suspend fun consumerInfoLocal(
        chainId: ChainId,
        accountId: AccountId,
    ): Result<ConsumerInfo?> {
        return localStorageSource.queryCatching(chainId) {
            val onChainInfo = metadata.resources.consumers.query(accountId)
            onChainInfo?.toDomain(accountId)
        }
    }

    override fun consumerInfoFlow(chainId: ChainId, accountId: AccountId) =
        remoteStorageSource.consumerInfoLocalFlow(chainId, accountId)

    override fun consumerInfoLocalFlow(chainId: ChainId, accountId: AccountId) =
        localStorageSource.consumerInfoLocalFlow(chainId, accountId)

    private fun StorageDataSource.consumerInfoLocalFlow(
        chainId: ChainId,
        accountId: AccountId,
    ): Flow<ConsumerInfo?> {
        return subscribe(chainId) {
            metadata.resources.consumers.observe(accountId)
                .map { it?.toDomain(accountId) }
        }
    }

    override suspend fun resolveConsumers(
        chainId: ChainId,
        accountIds: Collection<AccountId>,
    ): Result<Map<AccountId, ConsumerInfo>> {
        return remoteStorageSource.queryCatching(chainId) {
            metadata.resources.consumers.entries(accountIds)
                .mapValues {
                    it.value.toDomain(it.key)
                }
        }
    }

    private fun OnChainConsumerInfo.toDomain(accountId: AccountId) = ConsumerInfo(
        accountId = accountId,
        identifierKey = identifierKey.toDomain().getOrThrow(),
        liteUsername = liteUsername,
        fullUsername = fullUsername
    )
}
