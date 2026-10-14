package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.common.domain.model.AccountId

interface CallVisit {
    /**
     * Call that is currently visiting
     */
    val call: GenericCall.Instance

    /**
     * Origin's account id that this call has been dispatched with
     */
    val callOrigin: AccountId
}

class LeafCallVisit(
    override val call: GenericCall.Instance,
    override val callOrigin: AccountId
) : CallVisit

val CallVisit.isLeaf: Boolean
    get() = this is LeafCallVisit

interface BatchCallVisit : CallVisit {
    val batchedCalls: List<GenericCall.Instance>
}

interface MultisigCallVisit : CallVisit {
    val signatory: AccountId
        get() = callOrigin

    val otherSignatories: List<AccountId>

    val threshold: Int

    val multisig: AccountId

    val nestedCall: GenericCall.Instance
}

interface ProxyCallVisit : CallVisit {
    val proxy: AccountId
        get() = callOrigin

    val proxied: AccountId

    val nestedCall: GenericCall.Instance
}

fun CallVisit.requireLeafOrNull(): LeafCallVisit? {
    return this as? LeafCallVisit
}
