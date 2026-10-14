package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain

class NodeWithSaturatedUrl(
    val node: Chain.Node,
    val saturatedUrl: String,
)
