package io.paritytech.polkadotapp.chains.multiNetwork.connection.autobalance.strategy

import io.paritytech.polkadotapp.chains.multiNetwork.connection.NodeWithSaturatedUrl
import io.paritytech.polkadotapp.common.utils.cycle

class RoundRobinStrategy : AutoBalanceStrategy {
    override fun generateNodeSequence(defaultNodes: List<NodeWithSaturatedUrl>): Sequence<NodeWithSaturatedUrl> {
        return defaultNodes.cycle()
    }
}
