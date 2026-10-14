package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import java.math.BigInteger

class GetBlockHashRequest(blockNumber: BigInteger?) : RuntimeRequest(
    method = "chain_getBlockHash",
    params =
    listOfNotNull(
        blockNumber
    )
)
