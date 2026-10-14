package io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.common.domain.model.AccountId

interface CallTraversal {
    fun traverse(
        source: GenericCall.Instance,
        initialOrigin: AccountId,
        visitor: CallVisitor
    )
}

fun interface CallVisitor {
    fun visit(visit: CallVisit)
}

fun CallTraversal.collectAll(
    source: GenericCall.Instance,
    initialOrigin: AccountId,
): List<CallVisit> {
    return buildList {
        traverse(source, initialOrigin) {
            add(it)
        }
    }
}

fun CallTraversal.collectLeafs(
    source: GenericCall.Instance,
    initialOrigin: AccountId,
): List<CallVisit> {
    return buildList {
        traverse(source, initialOrigin) {
            if (it.isLeaf) {
                add(it)
            }
        }
    }
}
