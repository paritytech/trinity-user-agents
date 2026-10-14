package io.paritytech.polkadotapp.common.utils

inline fun <reified T : Throwable> Throwable.findInCauseChain(): T? {
    var current: Throwable? = this
    while (current != null) {
        if (current is T) return current
        current = current.cause
    }
    return null
}
