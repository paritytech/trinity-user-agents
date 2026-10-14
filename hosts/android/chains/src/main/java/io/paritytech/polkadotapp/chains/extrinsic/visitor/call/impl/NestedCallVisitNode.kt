package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.impl

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.extrinsic.visitor.ExtrinsicVisitorLogger
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api.CallVisit
import io.paritytech.polkadotapp.common.domain.model.AccountId

internal interface NestedCallVisitNode {
    fun canVisit(call: GenericCall.Instance): Boolean

    fun visit(call: GenericCall.Instance, context: CallVisitingContext)
}

internal interface CallVisitingContext {
    val origin: AccountId

    val logger: ExtrinsicVisitorLogger

    /**
     * Request parent to perform recursive visit of the given call
     */
    fun nestedVisit(visit: NestedCallVisit)

    /**
     * Call the supplied visitor with the given argument
     */
    fun visit(visit: CallVisit)
}

internal fun CallVisitingContext.nestedVisit(call: GenericCall.Instance, origin: AccountId) {
    nestedVisit(NestedCallVisit(call, origin))
}

/**
 * Version of [CallVisit] intended for nested usage
 *
 * @see [CallVisit]
 */
internal class NestedCallVisit(
    val call: GenericCall.Instance,
    val origin: AccountId
)
