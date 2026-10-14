package io.paritytech.polkadotapp.chains.multiNetwork.connection.autobalance.strategy

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Nodes.NodeSelectionStrategy
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.utils.flowOf
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class AutoBalanceStrategyProvider @Inject constructor() {
    private val roundRobin = RoundRobinStrategy()
    private val uniform = UniformStrategy()

    fun strategyFlowFor(
        chainId: ChainId,
        default: NodeSelectionStrategy,
    ): Flow<AutoBalanceStrategy> {
        return flowOf { strategyFor(default) }
    }

    fun strategyFor(config: NodeSelectionStrategy): AutoBalanceStrategy {
        return when (config) {
            NodeSelectionStrategy.ROUND_ROBIN -> roundRobin
            NodeSelectionStrategy.UNIFORM -> uniform
        }
    }
}
