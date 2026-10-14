package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class SubscribeNewHeads() : RuntimeRequest(
    method = "chain_subscribeNewHeads",
    params = emptyList()
)
