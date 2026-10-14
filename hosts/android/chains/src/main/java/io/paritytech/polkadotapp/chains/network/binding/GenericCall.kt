package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.common.data.substrate.cast

fun bindGenericCall(decoded: Any?): GenericCall.Instance {
    return decoded.cast()
}

fun bindGenericCallList(decoded: Any?): List<GenericCall.Instance> {
    return bindList(decoded, ::bindGenericCall)
}
