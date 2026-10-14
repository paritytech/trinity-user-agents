package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

object SubscribeRuntimeVersionRequest : RuntimeRequest(
    method = "state_subscribeRuntimeVersion",
    params = listOf()
)
