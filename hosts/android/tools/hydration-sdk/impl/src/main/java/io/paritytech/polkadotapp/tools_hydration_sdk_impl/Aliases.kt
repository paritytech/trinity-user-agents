package io.paritytech.polkadotapp.tools_hydration_sdk_impl

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.utils.graph.Graph
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge

typealias HydrationGraph = Graph<FullChainAssetId, HydrationSwapEdge>
