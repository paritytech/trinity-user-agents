package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nodes.proxy

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api.ProxyCallVisit
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.CallVisitingContext
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.NestedCallVisitNode
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nestedVisit
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindGenericCall
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.common.domain.model.AccountId

internal class ProxyCallNode : NestedCallVisitNode {
    private val proxyCalls = arrayOf("proxy", "proxyAnnounced")

    override fun canVisit(call: GenericCall.Instance): Boolean {
        return call.module.name == Modules.PROXY && call.function.name in proxyCalls
    }

    override fun visit(call: GenericCall.Instance, context: CallVisitingContext) {
        context.logger.info("Visiting proxy")

        val proxyVisit = RealProxyVisit(
            call = call,
            proxied = innerOrigin(call),
            nestedCall = innerCall(call),
            callOrigin = context.origin
        )

        context.visit(proxyVisit)
        context.nestedVisit(proxyVisit.nestedCall, proxyVisit.proxied)
    }

    private fun innerOrigin(proxyCall: GenericCall.Instance): AccountId {
        return bindAccountId(proxyCall.arguments["real"])
    }

    private fun innerCall(proxyCall: GenericCall.Instance): GenericCall.Instance {
        return bindGenericCall(proxyCall.arguments["call"])
    }

    private class RealProxyVisit(
        override val call: GenericCall.Instance,
        override val proxied: AccountId,
        override val nestedCall: GenericCall.Instance,
        override val callOrigin: AccountId
    ) : ProxyCallVisit
}
