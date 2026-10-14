package io.paritytech.polkadotapp.chains.network.binding

import io.paritytech.polkadotapp.chains.storage.source.query.DynamicInstanceBinder
import java.math.BigInteger

fun <T> collectionOf(itemBinder: DynamicInstanceBinder<T>): DynamicInstanceBinder<List<T>> {
    return { bindList(it, itemBinder) }
}

fun number(): DynamicInstanceBinder<BigInteger> = ::bindNumber
