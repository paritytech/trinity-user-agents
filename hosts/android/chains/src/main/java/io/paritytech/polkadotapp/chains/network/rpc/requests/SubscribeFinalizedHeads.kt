package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class SubscribeFinalizedHeads() : RuntimeRequest(
    method = "chain_subscribeFinalizedHeads",
    params = emptyList()
)
