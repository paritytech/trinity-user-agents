package io.paritytech.polkadotapp.feature_wallet_impl.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount

data class DigitalDollarBalance(
    val total: ChainAssetWithAmount,
    val ready: ChainAssetWithAmount
)
