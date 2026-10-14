package io.paritytech.polkadotapp.common.data.worker.stateMachine

import io.paritytech.polkadotapp.common.utils.typeTokenOf
import java.lang.reflect.Type

interface WorkerStateStore<S> {
    fun getRetryState(): S

    fun <P> getParams(type: Type): P
}

inline fun <reified P> WorkerStateStore<*>.getParams(): P {
    return getParams(typeTokenOf<P>().type)
}
