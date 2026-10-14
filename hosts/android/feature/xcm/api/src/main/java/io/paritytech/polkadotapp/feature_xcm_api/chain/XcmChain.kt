package io.paritytech.polkadotapp.feature_xcm_api.chain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import java.math.BigInteger

class XcmChain(
    val parachainId: BigInteger?,
    val chain: Chain
)

fun XcmChain.isRelay(): Boolean {
    return parachainId == null
}

fun XcmChain.isSystemChain(): Boolean {
    return parachainId != null && parachainId.toInt() in 1000 until 2000
}
