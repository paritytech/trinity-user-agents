package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class GetBlockRequest(blockHash: String? = null) : RuntimeRequest(
    method = "chain_getBlock",
    params =
    listOfNotNull(
        blockHash
    )
)
