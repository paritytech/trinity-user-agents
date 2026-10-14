package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nodes.batch

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.util.Modules

internal class BatchCallNode : BaseBatchNode() {
    override fun canVisit(call: GenericCall.Instance): Boolean {
        return call.module.name == Modules.UTILITY && call.function.name == "batch"
    }
}
