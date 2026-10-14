package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nodes.multisig

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api.MultisigCallVisit
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.CallVisitingContext
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.NestedCallVisitNode
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl.nestedVisit
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindGenericCall
import io.paritytech.polkadotapp.chains.network.binding.bindInt
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.common.domain.model.AccountId

internal class MultisigCallNode : NestedCallVisitNode {
    override fun canVisit(call: GenericCall.Instance): Boolean {
        return call.module.name == Modules.MULTISIG && call.function.name == "as_multi"
    }

    override fun visit(call: GenericCall.Instance, context: CallVisitingContext) {
        context.logger.info("Visiting multisig")

        val innerOriginInfo = extractMultisigOriginInfo(call, context.origin)
        val innerCall = extractInnerMultisigCall(call)

        val multisigVisit = RealMultisigCallVisit(
            call = call,
            callOrigin = context.origin,
            otherSignatories = innerOriginInfo.otherSignatories,
            threshold = innerOriginInfo.threshold,
            nestedCall = innerCall
        )

        context.visit(multisigVisit)
        context.nestedVisit(multisigVisit.nestedCall, multisigVisit.multisig)
    }

    private fun extractInnerMultisigCall(multisigCall: GenericCall.Instance): GenericCall.Instance {
        return bindGenericCall(multisigCall.arguments["call"])
    }

    private fun extractMultisigOriginInfo(call: GenericCall.Instance, parentOrigin: AccountId): MultisigOriginInfo {
        val threshold = bindInt(call.arguments["threshold"])
        val otherSignatories = bindList(call.arguments["other_signatories"], ::bindAccountId)

        return MultisigOriginInfo(threshold, otherSignatories)
    }

    private class MultisigOriginInfo(
        val threshold: Int,
        val otherSignatories: List<AccountId>,
    )

    private class RealMultisigCallVisit(
        override val call: GenericCall.Instance,
        override val callOrigin: AccountId,
        override val otherSignatories: List<AccountId>,
        override val threshold: Int,
        override val nestedCall: GenericCall.Instance,
    ) : MultisigCallVisit {
        override val multisig: AccountId = generateMultisigAddress(
            signatory = callOrigin,
            otherSignatories = otherSignatories,
            threshold = threshold
        )
    }
}
