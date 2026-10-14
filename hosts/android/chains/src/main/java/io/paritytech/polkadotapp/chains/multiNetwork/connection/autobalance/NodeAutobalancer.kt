package io.paritytech.polkadotapp.chains.multiNetwork.connection.autobalance

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ConnectionSecrets
import io.paritytech.polkadotapp.chains.multiNetwork.connection.NodeWithSaturatedUrl
import io.paritytech.polkadotapp.chains.multiNetwork.connection.autobalance.strategy.AutoBalanceStrategyProvider
import io.paritytech.polkadotapp.chains.multiNetwork.connection.saturateNodeUrls
import io.paritytech.polkadotapp.chains.util.wssNodes
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transform
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class NodeAutobalancer @Inject constructor(
    private val autobalanceStrategyProvider: AutoBalanceStrategyProvider,
    private val connectionSecrets: ConnectionSecrets,
) {
    fun connectionUrlFlow(
        chainId: ChainId,
        changeConnectionEventFlow: Flow<Unit>,
        availableNodesFlow: Flow<Chain.Nodes>,
    ): Flow<NodeWithSaturatedUrl?> {
        return availableNodesFlow.flatMapLatest { nodesConfig ->
            autobalanceStrategyProvider.strategyFlowFor(chainId, nodesConfig.nodeSelectionStrategy).transform { strategy ->
                Timber.d("Using ${nodesConfig.nodeSelectionStrategy} strategy for switching nodes in $chainId")

                val wssNodes = nodesConfig.wssNodes().saturateNodeUrls(connectionSecrets)

                if (wssNodes.isEmpty()) {
                    Timber.w("No wss nodes available for chain $chainId")

                    emit(null)
                }

                val nodeIterator = strategy.generateNodeSequence(wssNodes).iterator()

                emit(nodeIterator.next())

                val updates = changeConnectionEventFlow.map { nodeIterator.next() }
                emitAll(updates)
            }
        }
    }
}
