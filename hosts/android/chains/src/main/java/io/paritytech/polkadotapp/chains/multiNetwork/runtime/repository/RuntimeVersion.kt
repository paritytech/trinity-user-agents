package io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

class RuntimeVersion(
    val chainId: ChainId,
    val specVersion: Int,
)
