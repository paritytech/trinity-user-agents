package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nodes.peopleLite

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api.CallVisit
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.CallVisitingContext
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.NestedCallVisitNode
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nestedVisit
import io.paritytech.polkadotapp.chains.network.binding.bindGenericCall
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.instanceOf
import io.paritytech.polkadotapp.common.domain.model.AccountId

internal class DispatchAsSignerNode : NestedCallVisitNode {
    override fun canVisit(call: GenericCall.Instance): Boolean {
        return call.instanceOf(Modules.PEOPLE_LITE, "dispatch_as_signer")
    }

    override fun visit(call: GenericCall.Instance, context: CallVisitingContext) {
        context.logger.info("Visiting dispatch_as_signer")

        val visit = DispatchAsSignerVisit(
            call = call,
            callOrigin = context.origin,
            nestedCall = innerCall(call)
        )

        context.visit(visit)
        context.nestedVisit(visit.nestedCall, visit.callOrigin)
    }

    private fun innerCall(call: GenericCall.Instance): GenericCall.Instance {
        return bindGenericCall(call.arguments["call"])
    }

    private class DispatchAsSignerVisit(
        val nestedCall: GenericCall.Instance,
        override val call: GenericCall.Instance,
        override val callOrigin: AccountId
    ) : CallVisit
}
