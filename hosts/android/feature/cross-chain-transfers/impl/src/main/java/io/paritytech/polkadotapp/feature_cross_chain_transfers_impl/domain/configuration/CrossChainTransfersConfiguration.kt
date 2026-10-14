package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.ParaId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.TokenReserveRegistry

internal class CrossChainTransfersConfiguration(
    val parachainIds: Map<ChainId, ParaId>,
    val reserveRegistry: TokenReserveRegistry,
    val directions: CrossChainTransfersDirectionsConfiguration,
)
