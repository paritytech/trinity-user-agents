package io.paritytech.polkadotapp.feature_connection_status_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.common.utils.flowOfAll
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ChainHeadDataSource @Inject constructor(
    private val rpcCalls: RpcCalls,
) {
    // The substrate-sdk head subscription survives reconnects, so re-collecting does not miss blocks.
    fun bestBlockNumber(chainId: ChainId): Flow<Int> =
        flowOfAll { rpcCalls.subscribeNewHeads(chainId) }.map { it.number }
}
