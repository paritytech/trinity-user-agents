package io.paritytech.polkadotapp.chains.mapper

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.chain.RuntimeVersion
import io.paritytech.polkadotapp.database.model.chain.ChainRuntimeInfoLocal

fun ChainRuntimeInfoLocal.toRuntimeVersion(): RuntimeVersion? {
    return RuntimeVersion(
        specVersion = this.remoteVersion,
        transactionVersion = this.transactionVersion ?: return null
    )
}
