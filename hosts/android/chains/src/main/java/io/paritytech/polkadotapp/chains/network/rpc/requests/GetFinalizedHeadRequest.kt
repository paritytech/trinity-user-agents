package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

object GetFinalizedHeadRequest : RuntimeRequest(
    method = "chain_getFinalizedHead",
    params = emptyList()
)
