package io.paritytech.polkadotapp.chains.network.rpc.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.state.StateCallRequest
import io.paritytech.polkadotapp.chains.network.binding.BlockHash

class StateCallAtRequest(request: StateCallRequest, at: BlockHash) : RuntimeRequest(
    method = request.method,
    params = request.params + at,
)
