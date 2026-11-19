package io.paritytech.polkadotapp.feature_chain_resources_impl.data.repository

import io.paritytech.polkadotapp.chains.di.LocalSourceQualifier
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.scale.toDomain
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.model.ConsumerInfo
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.api.accountNames
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.api.dotNsGateway
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.model.DotNsOnChainConsumerInfo
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
            val onChainInfo = metadata.dotNsGateway.accountNames.query(accountId)
            onChainInfo?.toDomain(accountId)
        }
    }

    override fun consumerInfoFlow(chainId: ChainId, accountId: AccountId) =
        remoteStorageSource.consumerInfoFlow(chainId, accountId)

    override fun consumerInfoLocalFlow(chainId: ChainId, accountId: AccountId) =
        localStorageSource.consumerInfoFlow(chainId, accountId)

    private fun StorageDataSource.consumerInfoFlow(
        chainId: ChainId,
        accountId: AccountId,
    ): Flow<ConsumerInfo?> {
        return subscribe(chainId) {
            metadata.dotNsGateway.accountNames.observe(accountId)
                .map { it?.toDomain(accountId) }
        }
    }

    override suspend fun resolveConsumers(
        chainId: ChainId,
        accountIds: Collection<AccountId>,
    ): Result<Map<AccountId, ConsumerInfo>> {
        return remoteStorageSource.queryCatching(chainId) {
            metadata.dotNsGateway.accountNames.entries(accountIds)
                .mapNotNull { (accountId, onChainInfo) -> onChainInfo.toDomain(accountId)?.let { accountId to it } }
                .toMap()
        }
    }

    private fun DotNsOnChainConsumerInfo.toDomain(accountId: AccountId): ConsumerInfo? {
        val liteUsername = lite ?: return null
        val chat = full?.chat ?: liteUsername.chat ?: return null

        return ConsumerInfo(
            accountId = accountId,
            identifierKey = chat.toDomain().getOrThrow(),
            liteUsername = liteUsername.label,
            fullUsername = full?.label
        )
    }
}
