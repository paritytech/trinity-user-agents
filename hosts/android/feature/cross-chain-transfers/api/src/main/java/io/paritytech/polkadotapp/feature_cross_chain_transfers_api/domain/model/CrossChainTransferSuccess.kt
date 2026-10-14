package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

class CrossChainTransferSuccess(
    val receivedOnDestination: Balance
)
