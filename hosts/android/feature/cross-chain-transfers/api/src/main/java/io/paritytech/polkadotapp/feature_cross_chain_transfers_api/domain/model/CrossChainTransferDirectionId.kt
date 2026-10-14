package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.utils.graph.Edge

typealias CrossChainTransferDirectionId = Edge<FullChainAssetId>
