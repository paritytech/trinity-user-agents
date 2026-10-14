package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class NextAccountIndexRequest(accountAddress: String) : RuntimeRequest(
    method = "system_accountNextIndex",
    params =
    listOf(
        accountAddress
    )
)
