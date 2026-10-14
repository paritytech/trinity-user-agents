package io.paritytech.polkadotapp.common.utils

import kotlin.reflect.KProperty

class ResettableLazy<out T : Any>(private val initializer: () -> T) {
    private var value: T? = null

    @Synchronized
    operator fun getValue(thisRef: Any?, property: KProperty<*>): T =
        value ?: initializer().also { value = it }

    @Synchronized
    fun reset() {
        value = null
    }
}
