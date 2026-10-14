package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.repository

import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.binding.toBlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSources
import io.paritytech.polkadotapp.chains.storage.source.pickForDataConsistencyRequirement
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.chains.util.numberConstant
import io.paritytech.polkadotapp.chains.util.transactionStorage
import io.paritytech.polkadotapp.common.data.cache.CacheableDataConsistency
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.mapToUnit
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageRepository
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.TransactionStorageAuthorization
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.TransactionStorageAuthorizationScope
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.authorizations
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.canAccountPromote
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.transactionStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import javax.inject.Inject

class RealTransactionStorageRepository @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val storageDataSources: StorageDataSources,
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
    private val rpcCalls: RpcCalls,
) : TransactionStorageRepository {
    override suspend fun getAuthorization(
        chainId: ChainId,
        accountId: AccountId,
        consistency: CacheableDataConsistency,
    ): Result<TransactionStorageAuthorization?> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).queryCatching(chainId) {
            runtime.metadata.transactionStorage.authorizations.query(TransactionStorageAuthorizationScope.Account(accountId))
        }
    }

    override fun subscribeAuthorization(
        chainId: ChainId,
        accountId: AccountId,
        consistency: CacheableDataConsistency,
    ): Flow<TransactionStorageAuthorization?> {
        return storageDataSources.pickForDataConsistencyRequirement(consistency).subscribe(chainId) {
            runtime.metadata.transactionStorage.authorizations.observe(TransactionStorageAuthorizationScope.Account(accountId))
        }
    }

    override suspend fun authorizationPeriod(chainId: ChainId): Result<BlockNumber> {
        return runCatching {
            chainRegistry.withRuntime(chainId) {
                runtime.metadata.transactionStorage()
                    .numberConstant("AuthorizationPeriod")
                    .toBlockNumber()
            }
        }
    }

    override suspend fun canAccountPromote(chainId: ChainId, accountId: AccountId): Result<Boolean> {
        return runCatching {
            multiChainRuntimeCallsApi.forChain(chainId).canAccountPromote(accountId)
        }
    }

    override fun subscribeCanAccountPromote(chainId: ChainId, accountId: AccountId): Flow<Boolean> {
        return flow {
            val runtimeCalls = multiChainRuntimeCallsApi.forChain(chainId)
            emitAll(
                rpcCalls.subscribeNewHeads(chainId)
                    .mapToUnit()
                    .onStart { emit(Unit) }
                    .map { runtimeCalls.canAccountPromote(accountId) }
            )
        }
    }
}
