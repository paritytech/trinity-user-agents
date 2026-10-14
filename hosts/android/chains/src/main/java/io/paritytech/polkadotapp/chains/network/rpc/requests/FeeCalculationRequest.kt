package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class FeeCalculationRequest(extrinsicInHex: String) : RuntimeRequest(
    method = "payment_queryInfo",
    params = listOf(extrinsicInHex)
)
